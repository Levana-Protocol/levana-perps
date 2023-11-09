use std::{
    collections::HashMap,
    fmt::{Display, Write},
    sync::Arc,
};

use anyhow::Result;
use axum::async_trait;
use chrono::{DateTime, Duration, Utc};
use cosmos::{Address, HasAddress, TxBuilder, Wallet};
use cosmwasm_std::Decimal256;
use msg::{
    contracts::market::spot_price::{PythPriceServiceNetwork, SpotPriceConfig, SpotPriceFeedData},
    prelude::*,
};
use perps_exes::pyth::get_oracle_update_msg;
use shared::storage::MarketId;
use tokio::task::JoinSet;

use crate::{
    util::{
        markets::Market,
        oracle::{get_latest_price, OffchainPriceData},
    },
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};

use super::{crank_run::TriggerCrank, gas_check::GasCheckWallet, App, AppBuilder};

struct Worker {
    wallet: Arc<Wallet>,
    stats: HashMap<MarketId, ReasonStats>,
    /// This is the oldest feed publish time from the most recent successfully
    /// submitted price updates
    last_successful_price_publish_time: Option<DateTime<Utc>>,
    trigger_crank: TriggerCrank,
}

impl Worker {
    fn add_reason(
        &mut self,
        market: &MarketId,
        reason: &Option<(PriceUpdateReason, NeedsOracleUpdate)>,
    ) {
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
                    last_successful_price_publish_time: None,
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
        let last_successful_price_publish_time = worker.last_successful_price_publish_time;
        set.spawn(async move {
            let res = check_market_needs_price_update(
                &app,
                offchain_price_data,
                &market,
                last_successful_price_publish_time,
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
                successes.push(if let Some((reason, needs_oracle_update)) = reason {
                    if reason.is_too_frequent() {
                        format!("{}: Too frequent price updates, skipping", market.market_id)
                    } else {
                        if let NeedsOracleUpdate::Yes = needs_oracle_update {
                            any_needs_oracle_update = NeedsOracleUpdate::Yes;
                        }
                        markets_to_update.push(market.market.get_address());
                        format!("{}: Needs price update: {reason}", market.market_id)
                    }
                } else {
                    format!("{}: No price update needed", market.market_id)
                });
            }
            Err(e) => errors.push(format!(
                "{}: error checking if price update is needed: {e:?}",
                market.market_id
            )),
        }
    }

    // Now perform any oracle updates needed and trigger cranking as necessary
    if markets_to_update.is_empty() {
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

        if let Some(oldest_publish_time) = offchain_price_data.oldest_publish_time {
            worker.last_successful_price_publish_time = Some(oldest_publish_time);
            successes.push(format!(
                "Treating Pyth update timestamp as {oldest_publish_time}"
            ));
            let age = Utc::now().signed_duration_since(oldest_publish_time);
            if age.num_seconds() > 10 {
                successes.push(format!("Warning, Pyth update timestamp is older than expected, updates may fail. Age: {age}. Timestamp: {oldest_publish_time}"));
            }
        } else {
            successes.push("Warning, did not find a Pyth publish timestamp".to_owned());
        }
        for market in markets_to_update {
            worker.trigger_crank.trigger_crank(market).await;
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

enum NeedsOracleUpdate {
    Yes,
    No,
}

impl App {
    /// We don't bother with an oracle update if all feeds used by this contract are less than X seconds old
    async fn needs_oracle_update(&self, market: &Market) -> Result<NeedsOracleUpdate> {
        // Check that we actually use Pyth before making a smart contract query
        let uses_pyth = match &market.status.config.spot_price {
            SpotPriceConfig::Manual { .. } => false,
            SpotPriceConfig::Oracle {
                pyth: _,
                stride: _,
                feeds,
                feeds_usd,
            } => feeds.iter().chain(feeds_usd.iter()).any(|x| match x.data {
                SpotPriceFeedData::Constant { .. } => false,
                SpotPriceFeedData::Pyth { .. } => true,
                SpotPriceFeedData::Stride { .. } => false,
                SpotPriceFeedData::Sei { .. } => false,
                SpotPriceFeedData::Simple { .. } => false,
            }),
        };

        if !uses_pyth {
            return Ok(NeedsOracleUpdate::No);
        }

        let oracle_price = market.market.get_oracle_price().await?;

        let now = Utc::now();
        for x in oracle_price.pyth.values() {
            let updated = x.publish_time.try_into_chrono_datetime()?;
            let age = now.signed_duration_since(updated);
            if age.num_seconds() > MAX_ORACLE_AGE_SECONDS {
                return Ok(NeedsOracleUpdate::Yes);
            }
        }

        Ok(NeedsOracleUpdate::No)
    }
}

const MAX_ORACLE_AGE_SECONDS: i64 = 10;

#[tracing::instrument(skip_all)]
async fn check_market_needs_price_update(
    app: &App,
    offchain_price_data: Arc<OffchainPriceData>,
    market: &Market,
    last_successful_price_publish_time: Option<DateTime<Utc>>,
) -> Result<Option<(PriceUpdateReason, NeedsOracleUpdate)>> {
    let (oracle_price, _) = get_latest_price(&offchain_price_data, market).await?;
    let (_market_price, reason) = app
        .needs_price_update(market, oracle_price, last_successful_price_publish_time)
        .await?;
    Ok(reason)
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
    // contract as an immediate error. To our knowledge at time of writing, such
    // as situation should never happen. We may need to revise this in the
    // future for cases of known out-of-date prices, such as 24/5 markets, but
    // those can probably be better handled by not sending those updates
    // instead.

    match TxBuilder::default()
        .add_message(msg)
        .sign_and_broadcast(&app.cosmos, &worker.wallet)
        .await
    {
        Ok(res) => Ok(format!(
            "Prices updated in Pyth oracle contract with txhash {}",
            res.txhash
        )),
        Err(e) => {
            if app.is_osmosis_epoch() {
                Ok(format!("Unable to update Pyth oracle, but assuming it's because we're in the epoch: {e:?}"))
            } else {
                Err(e.into())
            }
        }
    }
}

type NeedPriceUpdateInner = (
    Option<PricePoint>,
    Option<(PriceUpdateReason, NeedsOracleUpdate)>,
);

impl App {
    /// Does the market need a price update?
    #[tracing::instrument(skip_all)]
    async fn needs_price_update(
        &self,
        market: &Market,
        oracle_price: PriceBaseInQuote,
        last_successful_price_publish_time: Option<DateTime<Utc>>,
    ) -> Result<NeedPriceUpdateInner> {
        let market_contract = &market.market;
        let market_price: PricePoint = match market_contract.current_price().await {
            Ok(price) => price,
            Err(e) => {
                let msg = format!("{e}");
                return if msg.contains("price_not_found") {
                    // Assume this is the first price being set
                    Ok((
                        None,
                        Some((PriceUpdateReason::NoPriceFound, NeedsOracleUpdate::Yes)),
                    ))
                } else {
                    Err(e.into())
                };
            }
        };

        let mut is_too_frequent = false;

        if let Some(publish_time) = market_price.publish_time {
            // Determine the logical "last update" by using both the
            // contract-derived price time and the most recent successful price
            // update we pushed. The reason for this is to avoid double-sending
            // price updates if one of the nodes reports an older price update.

            let publish_time = publish_time.try_into_chrono_datetime()?;
            let updated = (|| {
                let last_successful_price_publish_time = last_successful_price_publish_time?;
                if last_successful_price_publish_time < publish_time {
                    return None;
                }
                if Utc::now()
                    .signed_duration_since(last_successful_price_publish_time)
                    .num_seconds()
                    > self.config.max_price_age_secs.into()
                {
                    return None;
                }
                Some(last_successful_price_publish_time)
            })()
            .unwrap_or(publish_time);

            // Check 1: is the last price update too old?
            let age = Utc::now().signed_duration_since(updated);
            let age_secs = age.num_seconds();
            // Determine how old a price triggers a price update. We check
            // the defaults for the bots, the feeds themselves, and then add
            // a 10 second buffer to give time for transactions to land on
            // chain.
            let max_price_age_secs =
                market.max_price_age_with_default(self.config.max_price_age_secs) - 10;
            if age_secs > max_price_age_secs.into() {
                return Ok((
                    Some(market_price),
                    Some((
                        PriceUpdateReason::LastUpdateTooOld(age),
                        self.needs_oracle_update(market).await?,
                    )),
                ));
            }

            // Check 1a: if it's too new, we don't update, regardless of anything
            // else that might have happened. This is to prevent gas drainage.
            is_too_frequent = age_secs < self.config.min_price_age_secs.into();
        }

        // Check 2: has the price moved more than the allowed delta?
        let delta = oracle_price
            .into_non_zero()
            .raw()
            .checked_div(market_price.price_base.into_non_zero().raw())?
            .into_signed()
            .checked_sub(Signed::ONE)?
            .abs_unsigned();
        if delta >= self.config.max_allowed_price_delta {
            return Ok((
                Some(market_price),
                Some((
                    PriceUpdateReason::PriceDelta {
                        old: market_price.price_base,
                        new: oracle_price,
                        delta,
                        is_too_frequent,
                    },
                    self.needs_oracle_update(market).await?,
                )),
            ));
        }

        // Check 3: would any triggers happen from this price?
        // We save this for last since it requires a network round trip
        if market_contract.price_would_trigger(oracle_price).await? {
            // In this case we always do an oracle update, we want to make sure
            // this specific price point makes it into the contract.
            return Ok((
                Some(market_price),
                Some((PriceUpdateReason::Triggers, NeedsOracleUpdate::Yes)),
            ));
        }

        Ok((Some(market_price), None))
    }
}

enum PriceUpdateReason {
    LastUpdateTooOld(Duration),
    PriceDelta {
        old: PriceBaseInQuote,
        new: PriceBaseInQuote,
        delta: Decimal256,
        is_too_frequent: bool,
    },
    Triggers,
    NoPriceFound,
}

impl PriceUpdateReason {
    fn is_too_frequent(&self) -> bool {
        match self {
            PriceUpdateReason::LastUpdateTooOld(_) => false,
            PriceUpdateReason::PriceDelta {
                is_too_frequent, ..
            } => *is_too_frequent,
            PriceUpdateReason::Triggers => false,
            PriceUpdateReason::NoPriceFound => false,
        }
    }
}

#[derive(Debug)]
struct ReasonStats {
    market: MarketId,
    started_tracking: DateTime<Utc>,
    not_needed: u64,
    too_old: u64,
    delta: u64,
    delta_too_frequent: u64,
    triggers: u64,
    no_price_found: u64,
    oracle_update: u64,
}

impl Display for ReasonStats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let ReasonStats {
            market,
            started_tracking,
            not_needed,
            too_old,
            delta,
            delta_too_frequent,
            triggers,
            no_price_found,
            oracle_update,
        } = self;
        write!(f, "{market} {started_tracking}: not needed {not_needed}. too old {too_old}. Delta: {delta}. Delta too frequent: {delta_too_frequent}. Triggers: {triggers}. No price found: {no_price_found}. Oracle update: {oracle_update}.")
    }
}

impl ReasonStats {
    fn new(market: MarketId) -> Self {
        ReasonStats {
            started_tracking: Utc::now(),
            not_needed: 0,
            too_old: 0,
            delta: 0,
            delta_too_frequent: 0,
            triggers: 0,
            no_price_found: 0,
            oracle_update: 0,
            market,
        }
    }
    fn add_reason(&mut self, reason: &Option<(PriceUpdateReason, NeedsOracleUpdate)>) {
        let (reason, needs_oracle_update) = match reason {
            Some(reason) => reason,
            None => {
                self.not_needed += 1;
                return;
            }
        };
        match reason {
            PriceUpdateReason::LastUpdateTooOld(_) => self.too_old += 1,
            PriceUpdateReason::PriceDelta {
                is_too_frequent, ..
            } => {
                if *is_too_frequent {
                    self.delta_too_frequent += 1
                } else {
                    self.delta += 1
                }
            }
            PriceUpdateReason::Triggers => self.triggers += 1,
            PriceUpdateReason::NoPriceFound => self.no_price_found += 1,
        }
        if let NeedsOracleUpdate::Yes = needs_oracle_update {
            self.oracle_update += 1
        }
    }
}

impl Display for PriceUpdateReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PriceUpdateReason::LastUpdateTooOld(age) => write!(f, "Last update too old: {age}."),
            PriceUpdateReason::PriceDelta { old, new, delta, is_too_frequent } => write!(
                f,
                "Large price delta. Old: {old}. New: {new}. Delta: {delta}. Too frequent: {is_too_frequent}."
            ),
            PriceUpdateReason::Triggers => {
                write!(f, "Price would trigger positions and/or orders.")
            }
            PriceUpdateReason::NoPriceFound => write!(f, "No price point found."),
        }
    }
}

impl Market {
    fn max_price_age_with_default(&self, default_max_age: u32) -> u32 {
        let mut ret = default_max_age;
        match &self.status.config.spot_price {
            SpotPriceConfig::Manual { .. } => (),
            SpotPriceConfig::Oracle {
                feeds, feeds_usd, ..
            } => feeds
                .iter()
                .chain(feeds_usd.iter())
                .for_each(|feed| match &feed.data {
                    SpotPriceFeedData::Constant { .. } => (),
                    SpotPriceFeedData::Pyth {
                        age_tolerance_seconds,
                        ..
                    } => ret = ret.min(*age_tolerance_seconds),
                    SpotPriceFeedData::Stride {
                        age_tolerance_seconds,
                        ..
                    } => ret = ret.min(*age_tolerance_seconds),
                    SpotPriceFeedData::Sei { .. } => (),
                    SpotPriceFeedData::Simple {
                        age_tolerance_seconds,
                        ..
                    } => ret = ret.min(*age_tolerance_seconds),
                }),
        }
        ret
    }
}
