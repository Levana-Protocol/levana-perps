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
use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Address, CosmosTxResponse, HasAddress,
    TxBuilder, TxMessage, Wallet,
};
use perps_exes::pyth::get_oracle_update_msg;
use perpswap::storage::MarketId;
use perpswap::{
    contracts::market::{
        crank::CrankWorkInfo,
        spot_price::{PythPriceServiceNetwork, SpotPriceConfig},
    },
    prelude::*,
};
use tokio::task::JoinSet;

use crate::{
    config::NeedsPriceUpdateParams,
    util::{
        markets::Market,
        misc::track_tx_fees,
        oracle::{get_latest_price, LatestPrice, OffchainPriceData, PriceTooOld},
    },
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};

use super::{
    crank_run::TriggerCrank,
    high_gas::{HighGasTrigger, HighGasWork},
    App, AppBuilder, CrankTriggerReason, GasLevel,
};

struct Worker {
    stats: HashMap<MarketId, ReasonStats>,
    trigger_crank: TriggerCrank,
    high_gas_trigger: Option<HighGasTrigger>,
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
        if self.app.config.run_price_task {
            let high_gas_trigger = self.start_high_gas()?;
            self.watch_periodic(
                crate::watcher::TaskLabel::Price,
                Worker {
                    stats: HashMap::new(),
                    trigger_crank,
                    high_gas_trigger,
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

async fn run_price_update(worker: &mut Worker, app: Arc<App>) -> Result<WatchedTaskOutput> {
    let factory = app.get_factory_info().await;
    let mut successes = vec![];
    let mut errors = vec![];
    let mut markets_to_update = vec![];
    let mut any_needs_oracle_update = false;
    let mut max_gas_level = GasLevel::Normal;

    let begin_price_update = Instant::now();
    successes.push(format!(
        "Beginning run_price_update at {begin_price_update:?} ({})",
        Utc::now()
    ));

    // Load any offchain data, in batch, needed by the individual spot price configs
    let offchain_price_data = Arc::new(OffchainPriceData::load(&app, &factory.markets).await?);

    let got_price_data = Instant::now();
    successes.push(format!(
        "Time to get off chain price data: {:?}",
        got_price_data.saturating_duration_since(begin_price_update)
    ));

    // Now that we have the offchain data, parallelize the checking of
    // individual markets to see if we need to do a price update
    let mut set = JoinSet::new();
    for market in &factory.markets {
        let offchain_price_data = offchain_price_data.clone();
        let market = market.clone();
        let app = app.clone();
        set.spawn(async move {
            let res = check_market_needs_price_update(&app, offchain_price_data, &market).await;
            (market, res)
        });
    }

    let spawned = Instant::now();
    successes.push(format!(
        "Time to spawn market tasks: {:?}",
        spawned.saturating_duration_since(got_price_data)
    ));

    let mut last_iter = Instant::now();

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
                let now = Instant::now();
                worker.add_reason(&market.market_id, &reason);
                successes.push(format!(
                    "{}: {reason:?} (time: {:?})",
                    market.market_id,
                    now.saturating_duration_since(last_iter)
                ));
                last_iter = now;

                match reason {
                    ActionWithReason::NoWorkAvailable | ActionWithReason::PythPricesClosed => (),
                    ActionWithReason::PriceTooOld {
                        too_old:
                            PriceTooOld {
                                feed,
                                check_time,
                                publish_time,
                                age,
                                age_tolerance_seconds,
                            },
                    } => {
                        errors.push(format!("{}: price is too old. Check the price feed and try manual cranking in the frontend. Feed info: {feed}. Publish time: {publish_time}. Checked at: {check_time}. Age: {age}s. Tolerance: {age_tolerance_seconds}s.", market.market_id));
                    }
                    ActionWithReason::VolatileDiffTooLarge => {
                        errors.push(format!("{}: different in volatile price publish times is too high. Check the price feed and try manual cranking in the frontend.", market.market_id));
                    }
                    ActionWithReason::WorkNeeded(crank_trigger_reason) => {
                        if crank_trigger_reason.needs_price_update() {
                            any_needs_oracle_update = true;
                            max_gas_level = max_gas_level.max(crank_trigger_reason.gas_level());
                        }
                        markets_to_update.push((
                            market.market.get_address(),
                            market.market_id.clone(),
                            crank_trigger_reason,
                        ));
                    }
                }
            }
            Err(e) => {
                let now = Instant::now();

                errors.push(format!(
                    "{}: error checking if price update is needed: {e:?} (time: {:?})",
                    market.market_id,
                    now.saturating_duration_since(last_iter)
                ));
                last_iter = now;
            }
        }
    }

    successes.push(format!(
        "Total time to process all markets: {:?}",
        begin_price_update.elapsed()
    ));

    // Now perform any oracle updates needed and trigger cranking as necessary
    if markets_to_update.is_empty() {
        anyhow::ensure!(!any_needs_oracle_update);
        successes.push("No markets need updating".to_owned());
    } else {
        if any_needs_oracle_update {
            if let GasLevel::VeryHigh = max_gas_level {
                match &worker.high_gas_trigger {
                    Some(high_gas_trigger) => {
                        successes.push(format!(
                            "Passing the work to HighGas runner after {:?}",
                            begin_price_update.elapsed()
                        ));
                        high_gas_trigger
                            .set(HighGasWork {
                                offchain_price_data: offchain_price_data.clone(),
                                markets_to_update: markets_to_update.clone(),
                                queued: Instant::now(),
                            })
                            .await;
                    }
                    None => successes.push("Found high gas work, but we're on a chain that doesn't use a high gas wallet".to_owned())
                }
            }

            // Even if we do the Oracle UpdatePriceFeeds in the above
            // step, we don't want to wait for it to finish. So in the
            // below execution flow, we perform the Oracle
            // UpdatePriceFeeds again.
            let split_index = std::cmp::min(5, markets_to_update.len());
            let (markets_to_crank, remaining_markets_to_crank) =
                markets_to_update.split_at(split_index);
            let multi_message = MultiMessageEntity {
                markets: factory.markets.clone(),
                trigger: markets_to_crank.to_vec(),
            };

            let wallet = app.get_pool_wallet().await;
            let tx = construct_multi_message(&multi_message, &wallet, &app, &offchain_price_data)
                .await?;

            let result = process_cosmos_tx(tx, &app, &wallet).await;
            match result {
                Ok(res) => {
                    successes.push(res);
                    for (market, market_id, reason) in remaining_markets_to_crank.iter().cloned() {
                        worker
                            .trigger_crank
                            .trigger_crank(market, market_id, reason)
                            .await;
                    }
                }
                Err(e) => {
                    tracing::error!("Error: {e:?}\nRetrying...");

                    // Correct, not technically a success, but we want to display this info in the UI without forcing it to be treated as an error.
                    successes.push(format!("Error while doing multimessage price update, retrying with single message updates: {e:?}"));

                    let mut builder = TxBuilder::default();
                    if let Some(oracle_msg) = price_get_update_oracles_msg(
                        &wallet,
                        &app,
                        &multi_message.markets[..],
                        &offchain_price_data,
                    )
                    .await?
                    {
                        builder.add_message(oracle_msg);

                        let result = process_cosmos_tx(builder, &app, &wallet).await;
                        match result {
                            Ok(res) => {
                                successes.push(res);
                                for (market, market_id, reason) in markets_to_update.iter().cloned()
                                {
                                    worker
                                        .trigger_crank
                                        .trigger_crank(market, market_id, reason)
                                        .await;
                                }
                            }
                            Err(e2) => {
                                errors.push("Failed both multimessage and single message price update. Errors:".to_owned());
                                errors.push(format!("Multimessage error: {e:?}"));
                                errors.push(format!("Single message error: {e2:?}"));
                            }
                        }
                    }
                }
            }
        } else {
            successes.push("No markets needed an oracle update".to_owned());
        }

        if !any_needs_oracle_update {
            for (market, market_id, reason) in markets_to_update {
                worker
                    .trigger_crank
                    .trigger_crank(market, market_id, reason)
                    .await;
            }
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

/// This structure is used to compute a TxBuilder which is built and
/// we attempt to commit it in the blockchain.
pub(crate) struct MultiMessageEntity {
    /// Represents markets for which we need to perform oracle price
    /// update in the same transaction.
    pub(crate) markets: Vec<Market>,
    /// Represents markets for which we need to perform cranking as
    /// part of the same transaction.
    pub(crate) trigger: Vec<(Address, MarketId, CrankTriggerReason)>,
}

async fn process_cosmos_tx(tx: TxBuilder, app: &App, wallet: &Wallet) -> Result<String> {
    let response = tx.sign_and_broadcast_cosmos_tx(&app.cosmos, wallet).await;

    process_tx_result(app, wallet, &Instant::now(), response).await
}

async fn process_tx_result(
    app: &App,
    wallet: &Wallet,
    instant: &Instant,
    response: Result<CosmosTxResponse, cosmos::Error>,
) -> Result<String> {
    match response {
        Ok(res) => {
            track_tx_fees(app, wallet.get_address(), &res).await;
            Ok(format!(
                "Multi tx executed (Pyth update and cranking) with txhash {}, delay: {:?}",
                res.response.txhash,
                instant.elapsed(),
            ))
        }
        Err(e) => {
            if app.is_osmosis_epoch() {
                Ok(format!("Multi tx failed, but assuming it's because we're in the epoch: {e:?}, delay: {:?}", instant.elapsed()))
            } else if app.get_congested_info().await.is_congested() {
                Ok(format!("Multi tx failed, but assuming it's because Osmosis is congested: {e:?}, delay: {:?}", instant.elapsed()))
            } else {
                Err(e.into())
            }
        }
    }
}

/// Construct TxBuilder for both Oracle Update price feed as well as
/// to do the minimal cranking.
async fn construct_multi_message(
    message: &MultiMessageEntity,
    wallet: &Wallet,
    app: &App,
    offchain_price_data: &OffchainPriceData,
) -> Result<TxBuilder> {
    let mut builder = TxBuilder::default();
    if let Some(oracle_msg) =
        price_get_update_oracles_msg(wallet, app, &message.markets[..], offchain_price_data).await?
    {
        builder.add_message(oracle_msg);
    }
    for (market, _, _) in &message.trigger {
        let rewards = app
            .config
            .get_crank_rewards_wallet()
            .map(|a| a.get_address_string().into());

        builder.add_execute_message(
            market,
            wallet,
            vec![],
            MarketExecuteMsg::Crank {
                execs: Some(2),
                rewards: rewards.clone(),
            },
        )?;
    }
    Ok(builder)
}

#[derive(Debug)]
struct NeedsPriceUpdateInfo {
    /// The timestamp of the next pending deferred work item
    next_pending_deferred_work_item: Option<DateTime<Utc>>,
    /// The timestamp of the newest pending deferred work item
    newest_pending_deferred_work_item: Option<DateTime<Utc>>,
    /// The timestamp of the next liquifunding
    next_liquifunding: Option<DateTime<Utc>>,
    /// The latest price from on-chain oracle contract
    on_chain_oracle_price: PriceBaseInQuote,
    /// The latest publish time from on-chain oracle contract
    on_chain_oracle_publish_time: DateTime<Utc>,
    /// The latest price from on-chain market contract
    on_chain_market_price: PriceBaseInQuote,
    /// The latest publish time from on-chain market contract
    on_chain_market_publish_time: DateTime<Utc>,
    /// Latest off-chain price
    off_chain_price: PriceBaseInQuote,
    /// Latest off-chain publish time
    off_chain_publish_time: DateTime<Utc>,
    /// Does the contract report that there are crank work items?
    crank_work_available: Option<CrankWorkInfo>,
    /// Will the newest off-chain price update execute price triggers?
    price_will_trigger: bool,
    /// exposure_margin_ratio of the market; used to compare with the price delta to detect
    /// the moment the bots need to use very high gas wallet to try to
    /// land the oracle update for the LPs to be safe from late liquidations. The security
    /// concern of the price delta actually has an additional buffer of trading fees and
    /// liquidation margin for fees after settling pending fees.
    exposure_margin_ratio: Decimal256,
    /// Does the public time listed in the oracle violate the price feed's age tolerance?
    requires_pyth_update: bool,
}

#[derive(Debug)]
enum ActionWithReason {
    NoWorkAvailable,
    WorkNeeded(CrankTriggerReason),
    PythPricesClosed,
    PriceTooOld { too_old: PriceTooOld },
    VolatileDiffTooLarge,
}

impl NeedsPriceUpdateInfo {
    fn actions(&self, params: &NeedsPriceUpdateParams) -> Result<ActionWithReason> {
        // Keep the protocol lively: if on-chain price is too old or too
        // different from off-chain price, update price and crank.
        let oracle_to_off_chain_delta = (self.on_chain_oracle_price.into_number()
            - self.off_chain_price.into_number())?
        .abs_unsigned()
            / self.off_chain_price.into_non_zero().raw();
        let market_to_off_chain_delta = (self.on_chain_market_price.into_number()
            - self.off_chain_price.into_number())?
        .abs_unsigned()
            / self.off_chain_price.into_non_zero().raw();

        // If the new price would hit some new triggers, then we need to do a
        // price update and crank.
        if self.price_will_trigger {
            // Potential future optimization: only query this piece of data on-demand
            let very_high_threshold = self.exposure_margin_ratio;
            let high_threshold = params.on_off_chain_price_delta;

            // We know that we need to trigger a price update. Now we determine if the price delta is high enough that it warrants spending extra gas on Osmosis mainnet.
            let gas_level = if market_to_off_chain_delta > very_high_threshold
                || oracle_to_off_chain_delta > very_high_threshold
            {
                GasLevel::VeryHigh
            } else if market_to_off_chain_delta > high_threshold
                || oracle_to_off_chain_delta > high_threshold
            {
                GasLevel::High
            } else {
                GasLevel::Normal
            };
            return Ok(ActionWithReason::WorkNeeded(
                CrankTriggerReason::PriceWillTrigger { gas_level },
            ));
        }

        let on_chain_age = self
            .off_chain_publish_time
            .signed_duration_since(self.on_chain_market_publish_time);
        if on_chain_age > params.on_chain_publish_time_age_threshold {
            return Ok(ActionWithReason::WorkNeeded(
                CrankTriggerReason::OnChainTooOld {
                    on_chain_age,
                    off_chain_publish_time: self.off_chain_publish_time,
                    // here we provide the publish time from the market because it is the older of the two.
                    on_chain_oracle_publish_time: self.on_chain_market_publish_time,
                },
            ));
        }

        // See comment on needs_crank = true below.
        let mut needs_crank = false;

        // If the next liquifunding needs a price update, do it. Same for
        // deferred work, but we look at both the oldest and newest pending item to ensure
        // there's as little a gap between item creation and the price point that ends up
        // cranking it as possible.
        for timestamp in [
            self.next_pending_deferred_work_item,
            self.newest_pending_deferred_work_item,
            self.next_liquifunding,
        ]
        .into_iter()
        .flatten()
        {
            if self.on_chain_oracle_publish_time >= timestamp {
                // If the oracle price update timestamp is enough to make work available, do crank
                // even if there is no other reason to update the price.
                needs_crank = true;
            }
            if timestamp <= self.off_chain_publish_time
                && timestamp > self.on_chain_oracle_publish_time
            {
                return Ok(ActionWithReason::WorkNeeded(
                    CrankTriggerReason::CrankNeedsNewPrice {
                        work_item: timestamp,
                    },
                ));
            }
        }

        // No we know that pushing a price update won't trigger any new work to
        // become available. Now just check if there's already work available to process
        // and, if so, do a crank.
        if needs_crank || self.crank_work_available.is_some() {
            return Ok(ActionWithReason::WorkNeeded(
                CrankTriggerReason::CrankWorkAvailable {
                    requires_pyth_update: self.requires_pyth_update,
                },
            ));
        }

        // Nothing else caused a price update or crank, so no actions needed
        Ok(ActionWithReason::NoWorkAvailable)
    }
}

async fn check_market_needs_price_update(
    app: &App,
    offchain_price_data: Arc<OffchainPriceData>,
    market: &Market,
) -> Result<ActionWithReason> {
    if app
        .pyth_prices_closed(market.market.get_address(), &market.config)
        .await?
    {
        return Ok(ActionWithReason::PythPricesClosed);
    }
    match get_latest_price(&offchain_price_data, market).await? {
        LatestPrice::NoPriceInContract => Ok(ActionWithReason::WorkNeeded(
            CrankTriggerReason::NoPriceOnChain,
        )),
        LatestPrice::PriceTooOld { too_old } => Ok(ActionWithReason::PriceTooOld { too_old }),
        LatestPrice::VolatileDiffTooLarge => Ok(ActionWithReason::VolatileDiffTooLarge),
        LatestPrice::PricesFound {
            off_chain_price,
            off_chain_publish_time,
            on_chain_oracle_price,
            on_chain_oracle_publish_time,
            on_chain_price_point: market_price,
            requires_pyth_update,
        } => {
            let price_will_trigger = market.market.price_would_trigger(off_chain_price).await?;

            // Get a fresher status, not the cached one used above for checking Pyth prices.
            let status = market.market.status().await?;

            let info = NeedsPriceUpdateInfo {
                next_pending_deferred_work_item: status
                    .next_deferred_execution
                    .map(|x| x.try_into_chrono_datetime())
                    .transpose()?,
                newest_pending_deferred_work_item: status
                    .newest_deferred_execution
                    .map(|x| x.try_into_chrono_datetime())
                    .transpose()?,
                next_liquifunding: status
                    .next_liquifunding
                    .map(|x| x.try_into_chrono_datetime())
                    .transpose()?,
                off_chain_price,
                off_chain_publish_time,
                crank_work_available: status.next_crank.clone(),
                price_will_trigger,
                on_chain_oracle_price,
                on_chain_oracle_publish_time,
                on_chain_market_price: market_price.price_base,
                on_chain_market_publish_time: market_price.timestamp.try_into_chrono_datetime()?,
                exposure_margin_ratio: status.config.exposure_margin_ratio,
                requires_pyth_update,
            };

            info.actions(&app.config.needs_price_update_params)
        }
    }
}

pub(crate) async fn price_get_update_oracles_msg(
    wallet: &Wallet,
    app: &App,
    markets: &[Market],
    offchain_price_data: &OffchainPriceData,
) -> Result<Option<TxMessage>> {
    price_get_update_oracles_msg_raw(wallet, app, markets, offchain_price_data)
        .await
        .map(|msg| {
            msg.map(|msg| {
                let mut msg = TxMessage::from(msg);
                msg.set_description("Pyth price oracle update message");
                msg
            })
        })
}

async fn price_get_update_oracles_msg_raw(
    wallet: &Wallet,
    app: &App,
    markets: &[Market],
    offchain_price_data: &OffchainPriceData,
) -> Result<Option<MsgExecuteContract>> {
    if offchain_price_data.stable_ids.is_empty() && offchain_price_data.edge_ids.is_empty() {
        return Ok(None);
    }

    let mut stable_contract = None;
    let mut edge_contract = None;

    for market in markets {
        match &market.config.spot_price {
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

    match (stable_contract, edge_contract) {
        (None, None) => {
            anyhow::ensure!(offchain_price_data.stable_ids.is_empty());
            anyhow::ensure!(offchain_price_data.edge_ids.is_empty());
            Ok(None)
        }
        (Some(_), Some(_)) => anyhow::bail!("Cannot support both stable and edge Pyth contracts"),
        (Some(contract), None) => {
            anyhow::ensure!(edge_contract.is_none());
            anyhow::ensure!(offchain_price_data.edge_ids.is_empty());

            Ok(Some(
                get_oracle_update_msg(
                    &offchain_price_data.stable_ids,
                    &wallet,
                    &app.endpoint_stable,
                    &app.client,
                    &app.cosmos.make_contract(contract),
                )
                .await?,
            ))
        }
        (None, Some(contract)) => {
            anyhow::ensure!(stable_contract.is_none());
            anyhow::ensure!(offchain_price_data.stable_ids.is_empty());

            Ok(Some(
                get_oracle_update_msg(
                    &offchain_price_data.edge_ids,
                    &wallet,
                    &app.endpoint_edge,
                    &app.client,
                    &app.cosmos.make_contract(contract),
                )
                .await?,
            ))
        }
    }
}

#[derive(Debug)]
struct ReasonStats {
    market: MarketId,
    started_tracking: DateTime<Utc>,
    not_needed: u64,
    too_old: u64,
    normal_trigger: u64,
    high_trigger: u64,
    very_high_trigger: u64,
    crank_work_available: u64,
    more_work_found: u64,
    no_price_found: u64,
    deferred_needs_new_price: u64,
    pyth_prices_closed: u64,
    price_too_old: u64,
    volatile_diff_too_large: u64,
}

impl Display for ReasonStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let ReasonStats {
            market,
            started_tracking,
            not_needed,
            too_old,
            no_price_found,
            crank_work_available,
            more_work_found,
            deferred_needs_new_price,
            pyth_prices_closed,
            price_too_old,
            volatile_diff_too_large,
            normal_trigger,
            high_trigger,
            very_high_trigger,
        } = self;
        write!(f, "{market} {started_tracking}: not needed {not_needed}. too old {too_old}. Normal triggers: {normal_trigger}. High gas triggers: {high_trigger}. Very high gas triggers: {very_high_trigger}. No price found: {no_price_found}. Deferred execution w/price: {deferred_needs_new_price}. Pyth prices closed: {pyth_prices_closed}. Crank work available: {crank_work_available}. More work found: {more_work_found}. Price too old: {price_too_old}. Volatile diff too large: {volatile_diff_too_large}.")
    }
}

impl ReasonStats {
    fn new(market: MarketId) -> Self {
        ReasonStats {
            started_tracking: Utc::now(),
            not_needed: 0,
            too_old: 0,
            no_price_found: 0,
            market,
            crank_work_available: 0,
            more_work_found: 0,
            deferred_needs_new_price: 0,
            pyth_prices_closed: 0,
            price_too_old: 0,
            volatile_diff_too_large: 0,
            normal_trigger: 0,
            high_trigger: 0,
            very_high_trigger: 0,
        }
    }

    fn add_reason(&mut self, reason: &ActionWithReason) {
        match reason {
            ActionWithReason::NoWorkAvailable => self.not_needed += 1,
            ActionWithReason::PythPricesClosed => self.pyth_prices_closed += 1,
            ActionWithReason::PriceTooOld { .. } => self.price_too_old += 1,
            ActionWithReason::VolatileDiffTooLarge => self.volatile_diff_too_large += 1,
            ActionWithReason::WorkNeeded(reason) => self.add_work_reason(reason),
        }
    }

    fn add_work_reason(&mut self, reason: &CrankTriggerReason) {
        match reason {
            CrankTriggerReason::NoPriceOnChain => self.no_price_found += 1,
            CrankTriggerReason::OnChainTooOld { .. } => self.too_old += 1,
            CrankTriggerReason::CrankNeedsNewPrice { .. } => self.deferred_needs_new_price += 1,
            CrankTriggerReason::CrankWorkAvailable { .. } => self.crank_work_available += 1,
            CrankTriggerReason::PriceWillTrigger { gas_level } => match gas_level {
                GasLevel::Normal => self.normal_trigger += 1,
                GasLevel::High => self.high_trigger += 1,
                GasLevel::VeryHigh => self.very_high_trigger += 1,
            },
            CrankTriggerReason::MoreWorkFound => self.more_work_found += 1,
        }
    }
}
