pub(crate) mod pyth_market_hours;

use std::{
    collections::HashMap,
    fmt::{Display, Write},
    sync::Arc,
    time::Instant,
};

use anyhow::Result;
use axum::async_trait;
use chrono::{DateTime, Utc};
use cosmos::{Address, HasAddress, TxBuilder, Wallet};
use msg::{
    contracts::market::{
        crank::CrankWorkInfo,
        spot_price::{PythPriceServiceNetwork, SpotPriceConfig},
    },
    prelude::*,
};
use perps_exes::pyth::get_oracle_update_msg;
use shared::storage::MarketId;
use tokio::task::JoinSet;

use crate::{
    config::NeedsPriceUpdateParams,
    util::{
        markets::Market,
        misc::track_tx_fees,
        oracle::{get_latest_price, LatestPrice, OffchainPriceData},
    },
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};

use super::{
    crank_run::TriggerCrank, gas_check::GasCheckWallet, App, AppBuilder, CrankTriggerReason,
};

struct Worker {
    wallet: Arc<Wallet>,
    stats: HashMap<MarketId, ReasonStats>,
    /// The last time actions were taken for each market.
    last_action_taken: HashMap<MarketId, Instant>,
    trigger_crank: TriggerCrank,
}

impl Worker {
    fn add_reason(&mut self, market: &MarketId, reason: &ActionWithReason) {
        self.stats
            .entry(market.clone())
            .or_insert_with(|| ReasonStats::new(market.clone()))
            .add_reason(reason)
    }
}

/// Start the background thread to keep options pools up to date.
impl AppBuilder {
    pub(super) fn start_price(&mut self, trigger_crank: TriggerCrank) -> Result<()> {
        if let Some(price_wallet) = self.app.config.price_wallet.clone() {
            self.refill_gas(price_wallet.get_address(), GasCheckWallet::Price)?;
            self.watch_periodic(
                crate::watcher::TaskLabel::Price,
                Worker {
                    wallet: price_wallet,
                    stats: HashMap::new(),
                    last_action_taken: HashMap::new(),
                    trigger_crank,
                },
            )?;
        }
        Ok(())
    }
}

#[async_trait]
impl WatchedTask for Worker {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        run_price_update(self, app).await
    }
}

#[tracing::instrument(skip_all)]
async fn run_price_update(worker: &mut Worker, app: Arc<App>) -> Result<WatchedTaskOutput> {
    let factory = app.get_factory_info().await;
    let mut successes = vec![];
    let mut errors = vec![];
    let mut markets_to_update = vec![];
    let mut any_needs_oracle_update = NeedsOracleUpdate::No;

    // Load any offchain data, in batch, needed by the individual spot price configs
    let offchain_price_data = Arc::new(OffchainPriceData::load(&app, &factory.markets).await?);

    // Now that we have the offchain data, parallelize the checking of
    // individual markets to see if we need to do a price update
    let mut set = JoinSet::new();
    for market in &factory.markets {
        let offchain_price_data = offchain_price_data.clone();
        let market = market.clone();
        let app = app.clone();
        let last_action_taken = worker.last_action_taken.get(&market.market_id).copied();
        set.spawn(async move {
            let res = check_market_needs_price_update(
                &app,
                offchain_price_data,
                &market,
                last_action_taken,
            )
            .await;
            (market, res)
        });
    }

    // Wait for all the subtasks to complete
    while let Some(res_outer) = set.join_next().await {
        let (market, res) = match res_outer {
            Err(e) => {
                errors.push(format!("Code bug, panic occurred: {e:?}"));
                continue;
            }
            Ok(res) => res,
        };
        match res {
            Ok(reason) => {
                worker.add_reason(&market.market_id, &reason);
                successes.push(format!("{}: {reason:?}", market.market_id));

                match reason {
                    ActionWithReason::CooldownPeriod
                    | ActionWithReason::NoWorkAvailable
                    | ActionWithReason::PythPricesClosed
                    | ActionWithReason::OffChainPriceTooOld => (),
                    ActionWithReason::WorkNeeded(crank_trigger_reason) => {
                        if crank_trigger_reason.needs_price_update() {
                            any_needs_oracle_update = NeedsOracleUpdate::Yes;
                        }
                        markets_to_update.push((
                            market.market.get_address(),
                            market.market_id.clone(),
                            crank_trigger_reason,
                        ));
                    }
                }
            }
            Err(e) => errors.push(format!(
                "{}: error checking if price update is needed: {e:?}",
                market.market_id
            )),
        }
    }

    // Now perform any oracle updates needed and trigger cranking as necessary
    if markets_to_update.is_empty() {
        anyhow::ensure!(any_needs_oracle_update == NeedsOracleUpdate::No);
        successes.push("No markets need updating".to_owned());
    } else {
        match any_needs_oracle_update {
            NeedsOracleUpdate::Yes => {
                successes.push(
                    update_oracles(worker, &app, &factory.markets, &offchain_price_data).await?,
                );
            }
            NeedsOracleUpdate::No => {
                successes.push("No markets needed an oracle update".to_owned());
            }
        }

        for (market, market_id, reason) in markets_to_update {
            worker
                .trigger_crank
                .trigger_crank(market, market_id, reason)
                .await;
        }
    }

    // Add the stats
    for (market_id, reason_stats) in &worker.stats {
        successes.push(format!("Stats {market_id}: {reason_stats}"));
    }

    // Generate the correct output
    let is_error = !errors.is_empty();
    let mut msg = String::new();
    for line in errors.into_iter().chain(successes.into_iter()) {
        writeln!(&mut msg, "{line}")?;
    }

    if is_error {
        Err(anyhow::anyhow!({ msg }))
    } else {
        Ok(WatchedTaskOutput::new(msg))
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum NeedsOracleUpdate {
    Yes,
    No,
}

#[derive(Debug)]
struct NeedsPriceUpdateInfo {
    /// Last time we took any actions for this market.
    last_action: Option<Instant>,
    /// The timestamp of the next pending deferred work item
    next_pending_deferred_work_item: Option<DateTime<Utc>>,
    /// The latest price from on-chain data only
    on_chain_price: PriceBaseInQuote,
    /// The latest publish time from on-chain data only
    on_chain_publish_time: DateTime<Utc>,
    /// Latest off-chain price
    off_chain_price: PriceBaseInQuote,
    /// Latest off-chain publish time
    off_chain_publish_time: DateTime<Utc>,
    /// Does the contract report that there are crank work items?
    crank_work_available: Option<CrankWorkInfo>,
    /// Will the newest off-chain price update execute price triggers?
    price_will_trigger: bool,
}

#[derive(Debug)]
enum ActionWithReason {
    CooldownPeriod,
    NoWorkAvailable,
    WorkNeeded(CrankTriggerReason),
    PythPricesClosed,
    OffChainPriceTooOld,
}

impl NeedsPriceUpdateInfo {
    fn actions(&self, params: &NeedsPriceUpdateParams) -> ActionWithReason {
        if let Some(last_action) = self.last_action {
            if let Some(age) = Instant::now().checked_duration_since(last_action) {
                if age < params.action_cooldown_period {
                    return ActionWithReason::CooldownPeriod;
                }
            }
        }

        // Keep the protocol lively: if on-chain price is too old or too
        // different from off-chain price, update price and crank.
        let on_chain_age = self
            .off_chain_publish_time
            .signed_duration_since(self.on_chain_publish_time);
        if on_chain_age > params.on_chain_publish_time_age_threshold {
            return ActionWithReason::WorkNeeded(CrankTriggerReason::OnChainTooOld {
                on_chain_age,
                off_chain_publish_time: self.off_chain_publish_time,
                on_chain_oracle_publish_time: self.on_chain_publish_time,
            });
        }
        let on_off_chain_delta =
            (self.on_chain_price.into_number() - self.off_chain_price.into_number()).abs_unsigned()
                / self.off_chain_price.into_non_zero().raw();
        if on_off_chain_delta > params.on_off_chain_price_delta {
            return ActionWithReason::WorkNeeded(CrankTriggerReason::LargePriceDelta {
                on_off_chain_delta,
                on_chain_oracle_price: self.on_chain_price,
                off_chain_price: self.off_chain_price,
            });
        }

        // If there are deferred work items...
        if let Some(deferred_work_item) = self.next_pending_deferred_work_item {
            // Do we need a new publish time in the oracle to crank this?
            if self.on_chain_publish_time < deferred_work_item {
                // New price is needed, do we have it?
                if self.off_chain_publish_time >= deferred_work_item {
                    return ActionWithReason::WorkNeeded(
                        CrankTriggerReason::DeferredNeedsNewPrice {
                            on_chain_oracle_publish_time: self.on_chain_publish_time,
                            deferred_work_item,
                        },
                    );
                }
            } else {
                // No new price is needed, so we can just crank and make progress.
                return ActionWithReason::WorkNeeded(CrankTriggerReason::DeferredWorkAvailable {
                    on_chain_oracle_publish_time: self.on_chain_publish_time,
                    deferred_work_item,
                });
            }
        }

        // If there are crank work items available, then just crank
        if let Some(crank_work) = &self.crank_work_available {
            if let CrankWorkInfo::DeferredExec { .. } = crank_work {
                tracing::error!("This case should never happen, found deferred exec crank work but didn't handle it with other deferred work items");
            } else {
                return ActionWithReason::WorkNeeded(CrankTriggerReason::CrankWorkAvailable);
            }
        }

        // No other work is immediately available. Now we check if a new price
        // update would hit any triggers. We do this at the end to avoid unnecessarily
        // spamming oracle price updates while still processing earlier items in the queue.
        if self.price_will_trigger {
            // Potential future optimization: only query this piece of data on-demand
            return ActionWithReason::WorkNeeded(CrankTriggerReason::PriceWillTrigger);
        }

        // Nothing else caused a price update or crank, so no actions needed
        ActionWithReason::NoWorkAvailable
    }
}

#[tracing::instrument(skip_all)]
async fn check_market_needs_price_update(
    app: &App,
    offchain_price_data: Arc<OffchainPriceData>,
    market: &Market,
    last_action_taken: Option<Instant>,
) -> Result<ActionWithReason> {
    if app
        .pyth_prices_closed(market.market.get_address(), Some(&market.status))
        .await?
    {
        return Ok(ActionWithReason::PythPricesClosed);
    }
    match get_latest_price(&offchain_price_data, market).await? {
        LatestPrice::NoPriceInContract => Ok(ActionWithReason::WorkNeeded(
            CrankTriggerReason::NoPriceOnChain,
        )),
        LatestPrice::PriceTooOld => Ok(ActionWithReason::OffChainPriceTooOld),
        LatestPrice::PricesFound {
            off_chain_price,
            off_chain_publish_time,
            on_chain_price,
            on_chain_publish_time,
        } => {
            let price_will_trigger = market.market.price_would_trigger(off_chain_price).await?;

            let info = NeedsPriceUpdateInfo {
                last_action: last_action_taken,
                next_pending_deferred_work_item: market
                    .status
                    .next_deferred_execution
                    .map(|x| x.try_into_chrono_datetime())
                    .transpose()?,
                off_chain_price,
                off_chain_publish_time,
                crank_work_available: market.status.next_crank.clone(),
                price_will_trigger,
                on_chain_price,
                on_chain_publish_time,
            };

            Ok(info.actions(&app.config.needs_price_update_params))
        }
    }
}

async fn update_oracles(
    worker: &mut Worker,
    app: &App,
    markets: &[Market],
    offchain_price_data: &OffchainPriceData,
) -> Result<String> {
    if offchain_price_data.stable_ids.is_empty() && offchain_price_data.edge_ids.is_empty() {
        return Ok("No Pyth IDs found, no Pyth oracle update needed".to_owned());
    }
    let mut stable_contract = None;
    let mut edge_contract = None;

    for market in markets {
        match &market.status.config.spot_price {
            SpotPriceConfig::Manual { .. } => (),
            SpotPriceConfig::Oracle { pyth: None, .. } => (),
            SpotPriceConfig::Oracle {
                pyth: Some(pyth), ..
            } => {
                let contract: Address = pyth.contract_address.as_str().parse()?;
                match pyth.network {
                    PythPriceServiceNetwork::Stable => {
                        anyhow::ensure!(
                            edge_contract.is_none(),
                            "Cannot support both stable and edge Pyth contracts"
                        );
                        if let Some(curr) = stable_contract {
                            anyhow::ensure!(
                                curr == contract,
                                "Different Pyth contract addresses found: {curr} {contract}"
                            );
                        }
                        stable_contract = Some(contract);
                    }
                    PythPriceServiceNetwork::Edge => {
                        anyhow::ensure!(
                            stable_contract.is_none(),
                            "Cannot support both stable and edge Pyth contracts"
                        );
                        if let Some(curr) = edge_contract {
                            anyhow::ensure!(
                                curr == contract,
                                "Different Pyth contract addresses found: {curr} {contract}"
                            );
                        }
                        edge_contract = Some(contract);
                    }
                };
            }
        }
    }

    let msg = match (stable_contract, edge_contract) {
        (None, None) => {
            anyhow::ensure!(offchain_price_data.stable_ids.is_empty());
            anyhow::ensure!(offchain_price_data.edge_ids.is_empty());
            return Ok("No Pyth price feeds found to update".to_owned());
        }
        (Some(_), Some(_)) => anyhow::bail!("Cannot support both stable and edge Pyth contracts"),
        (Some(contract), None) => {
            anyhow::ensure!(edge_contract.is_none());
            anyhow::ensure!(offchain_price_data.edge_ids.is_empty());

            get_oracle_update_msg(
                &offchain_price_data.stable_ids,
                &*worker.wallet,
                &app.endpoint_stable,
                &app.client,
                &app.cosmos.make_contract(contract),
            )
            .await?
        }
        (None, Some(contract)) => {
            anyhow::ensure!(stable_contract.is_none());
            anyhow::ensure!(offchain_price_data.stable_ids.is_empty());

            get_oracle_update_msg(
                &offchain_price_data.edge_ids,
                &*worker.wallet,
                &app.endpoint_edge,
                &app.client,
                &app.cosmos.make_contract(contract),
            )
            .await?
        }
    };

    // Previously, with PERP-1702, we had some logic to ignore some errors from
    // out-of-date prices. However, since we're no longer updating the market
    // contract here, that's not relevant, so that logic has been removed.
    // Overall: we want to treat _any_ failure to update prices in the Pyth
    // contract as an immediate error. The one exception for now: if Pyth
    // reports that prices for this market are currently closed, we ignore such
    // an error.

    match TxBuilder::default()
        .add_message(msg.clone())
        .sign_and_broadcast_cosmos_tx(&app.cosmos, &worker.wallet)
        .await
    {
        Ok(res) => {
            track_tx_fees(app, worker.wallet.get_address(), &res).await;
            Ok(format!(
                "Prices updated in Pyth oracle contract with txhash {}",
                res.response.txhash
            ))
        }
        Err(e) => {
            if app.is_osmosis_epoch() {
                Ok(format!("Unable to update Pyth oracle, but assuming it's because we're in the epoch: {e:?}"))
            } else {
                Err(e.into())
            }
        }
    }
}

#[derive(Debug)]
struct ReasonStats {
    market: MarketId,
    started_tracking: DateTime<Utc>,
    oracle_update: u64,
    cooldown: u64,
    not_needed: u64,
    too_old: u64,
    delta: u64,
    triggers: u64,
    crank_work_available: u64,
    more_work_found: u64,
    no_price_found: u64,
    deferred_needs_new_price: u64,
    deferred_work_available: u64,
    pyth_prices_closed: u64,
    offchain_price_too_old: u64,
}

impl Display for ReasonStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let ReasonStats {
            market,
            cooldown,
            started_tracking,
            not_needed,
            too_old,
            delta,
            triggers,
            no_price_found,
            oracle_update,
            crank_work_available,
            more_work_found,
            deferred_needs_new_price,
            deferred_work_available,
            pyth_prices_closed,
            offchain_price_too_old,
        } = self;
        write!(f, "{market} {started_tracking}: not needed {not_needed}. too old {too_old}. Delta: {delta}. Cooldown: {cooldown}. Triggers: {triggers}. No price found: {no_price_found}. Oracle update: {oracle_update}. Deferred execution w/price: {deferred_needs_new_price}. Deferred w/o price: {deferred_work_available}. Pyth prices closed: {pyth_prices_closed}. Crank work available: {crank_work_available}. More work found: {more_work_found}. Offchain price too old: {offchain_price_too_old}.")
    }
}

impl ReasonStats {
    fn new(market: MarketId) -> Self {
        ReasonStats {
            started_tracking: Utc::now(),
            cooldown: 0,
            not_needed: 0,
            too_old: 0,
            delta: 0,
            triggers: 0,
            no_price_found: 0,
            oracle_update: 0,
            market,
            crank_work_available: 0,
            more_work_found: 0,
            deferred_needs_new_price: 0,
            deferred_work_available: 0,
            pyth_prices_closed: 0,
            offchain_price_too_old: 0,
        }
    }

    fn add_reason(&mut self, reason: &ActionWithReason) {
        match reason {
            ActionWithReason::CooldownPeriod => self.cooldown += 1,
            ActionWithReason::NoWorkAvailable => self.not_needed += 1,
            ActionWithReason::PythPricesClosed => self.pyth_prices_closed += 1,
            ActionWithReason::OffChainPriceTooOld => self.offchain_price_too_old += 1,
            ActionWithReason::WorkNeeded(reason) => {
                if reason.needs_price_update() {
                    self.oracle_update += 1;
                }
                self.add_work_reason(reason);
            }
        }
    }

    fn add_work_reason(&mut self, reason: &CrankTriggerReason) {
        match reason {
            CrankTriggerReason::NoPriceOnChain => self.no_price_found += 1,
            CrankTriggerReason::OnChainTooOld { .. } => self.too_old += 1,
            CrankTriggerReason::LargePriceDelta { .. } => self.delta += 1,
            CrankTriggerReason::DeferredNeedsNewPrice { .. } => self.deferred_needs_new_price += 1,
            CrankTriggerReason::DeferredWorkAvailable { .. } => self.deferred_work_available += 1,
            CrankTriggerReason::CrankWorkAvailable => self.crank_work_available += 1,
            CrankTriggerReason::PriceWillTrigger => self.triggers += 1,
            CrankTriggerReason::MoreWorkFound => self.more_work_found += 1,
        }
    }
}
