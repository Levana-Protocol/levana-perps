use std::{collections::HashMap, fmt::Display, str::FromStr, sync::Arc};

use anyhow::Result;
use axum::async_trait;
use chrono::{DateTime, Duration, Utc};
use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Address, HasAddress, TxBuilder, Wallet,
};
use cosmwasm_std::Decimal256;
use msg::{
    contracts::pyth_bridge::entry::FeedType,
    prelude::{PriceBaseInQuote, Signed, UnsignedDecimal},
};
use perps_exes::pyth::{get_latest_price, get_oracle_update_msg};
use shared::storage::MarketId;

use crate::{
    config::BotConfigByType,
    util::{markets::Market, oracle::Pyth},
    watcher::{WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, gas_check::GasCheckWallet, App, AppBuilder};

struct Worker {
    wallet: Arc<Wallet>,
    stats: HashMap<MarketId, ReasonStats>,
    last_successful_price_times: HashMap<MarketId, DateTime<Utc>>,
}

impl Worker {
    fn add_reason(&mut self, market: &MarketId, reason: &Option<PriceUpdateReason>) {
        self.stats
            .entry(market.clone())
            .or_insert_with(|| ReasonStats::new(market.clone()))
            .add_reason(reason)
    }
}

/// Start the background thread to keep options pools up to date.
impl AppBuilder {
    pub(super) fn start_price(&mut self) -> Result<()> {
        if let Some(price_wallet) = self.app.config.price_wallet.clone() {
            match &self.app.config.by_type {
                BotConfigByType::Testnet { inner } => {
                    let inner = inner.clone();
                    self.refill_gas(&inner, *price_wallet.address(), GasCheckWallet::Price)?;
                }
                BotConfigByType::Mainnet { inner } => {
                    self.alert_on_low_gas(
                        *price_wallet.address(),
                        GasCheckWallet::Price,
                        inner.min_gas_price,
                    )?;
                }
            }
            self.watch_periodic(
                crate::watcher::TaskLabel::Price,
                Worker {
                    wallet: price_wallet,
                    stats: HashMap::new(),
                    last_successful_price_times: HashMap::new(),
                },
            )?;
        }
        Ok(())
    }
}

#[async_trait]
impl WatchedTaskPerMarket for Worker {
    async fn run_single_market(
        &mut self,
        app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        let message = app.single_update(market, self).await?;
        let stats = self
            .stats
            .entry(market.market_id.clone())
            .or_insert_with(|| ReasonStats::new(market.market_id.clone()));
        Ok(WatchedTaskOutput {
            skip_delay: false,
            message: format!("{message}. {stats}"),
        })
    }
}

impl App {
    async fn single_update(&self, market: &Market, worker: &mut Worker) -> Result<String> {
        let mut statuses = vec![];
        let mut builder = TxBuilder::default();

        // Load it up each time in case there are config changes, but we could
        // theoretically optimize this by doing it at load time instead.
        let bridge_addr = Address::from_str(&market.price_admin)?;
        let pyth = Pyth::new(&self.cosmos, bridge_addr, market.market_id.clone()).await?;

        let (latest_price, _) = get_latest_price(
            &self.client,
            &pyth.market_price_feeds,
            match pyth.feed_type {
                FeedType::Stable => &self.endpoints_stable,
                FeedType::Edge => &self.endpoints_edge,
            },
        )
        .await?;
        let reason = self
            .needs_price_update(
                market,
                &pyth,
                latest_price,
                worker
                    .last_successful_price_times
                    .get(&market.market_id)
                    .copied(),
            )
            .await?;
        worker.add_reason(&market.market_id, &reason);
        if let Some(reason) = reason {
            if reason.is_too_frequent() {
                return Ok("Too frequent price updates, skipping".to_owned());
            }
            let msgs = self
                .get_txs_pyth(&worker.wallet, &pyth, self.config.execs_per_price)
                .await?;
            for msg in msgs {
                builder.add_message_mut(msg);
            }
            statuses.push(format!("Needs Pyth update: {reason}"));
        } else {
            return Ok("No pyth price update needed".to_owned());
        }

        // Take the crank lock for the rest of the execution
        let crank_lock = self.crank_lock.lock().await;

        let res = match builder
            .sign_and_broadcast(&self.cosmos, &worker.wallet)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                // PERP-1702: If the price is too old, only complain after a
                // longer period of time to avoid spurious alerts.

                // Hacky way to check if we're getting this error, we could
                // parse the error correctly, but this is Good Enough.
                if !format!("{e:?}").contains("price_too_old") {
                    return Err(e);
                }

                // OK, it was a too old error. Let's find out when the last price update was for the contract.
                let last_update = pyth
                    .prev_market_price_timestamp(&market.market_id)
                    .await?
                    .try_into_chrono_datetime()?;
                let now = Utc::now();
                let age = now - last_update;
                if u32::try_from(age.num_seconds())? > self.config.price_age_alert_threshold_secs {
                    return Err(e);
                } else {
                    return Ok(format!(
                        "Ignoring failed price update. Price age in contract is: {age}"
                    ));
                }
            }
        };

        // the storage should have been updated from the above transaction
        match pyth.prev_market_price_timestamp(&market.market_id).await {
            Ok(timestamp) => {
                let datetime = timestamp.try_into_chrono_datetime()?;
                worker
                    .last_successful_price_times
                    .insert(market.market_id.clone(), datetime);
            }
            Err(e) => {
                log::error!("Unable to parse price tx response timestamp: {e:?}");
            }
        }

        std::mem::drop(crank_lock);

        // just for logging pyth prices
        let msg = match pyth.query_price(120).await {
            Ok(market_price) => format!("Updated pyth price: {market_price:?}"),
            Err(e) => format!("query_price failed, ignoring: {e:?}."),
        };
        statuses.push(msg);

        if !res.data.is_empty() {
            statuses.push(format!("Response data from contracts: {}", res.data));
        }

        statuses.push(format!(
            "Prices updated in oracles with txhash {}",
            res.txhash
        ));

        Ok(statuses.join("\n"))
    }

    /// Does the market need a price update?
    async fn needs_price_update(
        &self,
        market: &Market,
        pyth: &Pyth,
        latest_price: PriceBaseInQuote,
        last_successful_price_time: Option<DateTime<Utc>>,
    ) -> Result<Option<PriceUpdateReason>> {
        let market_id = &market.market_id;
        let market = &market.market;
        let price_base = match market.current_price().await {
            Ok(price) => price.price_base,
            Err(e) => {
                let msg = format!("{e}");
                return if msg.contains("price_not_found") {
                    // Assume this is the first price being set
                    Ok(Some(PriceUpdateReason::NoPriceFound))
                } else {
                    Err(e)
                };
            }
        };

        let price_time = pyth.prev_market_price_timestamp(market_id).await?;

        // Determine the logical "last update" by using both the
        // contract-derived price time and the most recent successfully price
        // update we pushed. The reason for this is to avoid double-sending
        // price updates if one of the nodes reports an older price update.
        let price_time = price_time.try_into_chrono_datetime()?;
        let updated = (|| {
            let last_successful_price_time = last_successful_price_time?;
            if last_successful_price_time < price_time {
                return None;
            }
            if Utc::now()
                .signed_duration_since(last_successful_price_time)
                .num_seconds()
                > self.config.max_price_age_secs.into()
            {
                return None;
            }
            Some(last_successful_price_time)
        })()
        .unwrap_or(price_time);

        // Check 1: is the last price update too old?
        let age = Utc::now().signed_duration_since(updated);
        let age_secs = age.num_seconds();
        if age_secs > self.config.max_price_age_secs.into() {
            return Ok(Some(PriceUpdateReason::LastUpdateTooOld(age)));
        }

        log::info!("age: {}, price_time: {}", age, price_time);

        // Check 1a: if it's too new, we don't update, regardless of anything
        // else that might have happened. This is to prevent gas drainage.
        let is_too_frequent = age_secs < self.config.min_price_age_secs.into();

        // Check 2: has the price moved more than the allowed delta?
        let delta = latest_price
            .into_non_zero()
            .raw()
            .checked_div(price_base.into_non_zero().raw())?
            .into_signed()
            .checked_sub(Signed::ONE)?
            .abs_unsigned();
        if delta >= self.config.max_allowed_price_delta {
            return Ok(Some(PriceUpdateReason::PriceDelta {
                old: price_base,
                new: latest_price,
                delta,
                is_too_frequent,
            }));
        }

        // Check 3: would any triggers happen from this price?
        // We save this for last since it requires a network round trip
        if market.price_would_trigger(latest_price).await? {
            return Ok(Some(PriceUpdateReason::Triggers { is_too_frequent }));
        }

        Ok(None)
    }

    async fn get_txs_pyth(
        &self,
        wallet: &Wallet,
        pyth: &Pyth,
        execs: Option<u32>,
    ) -> Result<Vec<MsgExecuteContract>> {
        let oracle_msg = get_oracle_update_msg(
            &pyth.market_price_feeds,
            &wallet,
            match pyth.feed_type {
                FeedType::Stable => &self.endpoints_stable,
                FeedType::Edge => &self.endpoints_edge,
            },
            &self.client,
            &pyth.oracle,
        )
        .await?;
        let bridge_msg = pyth
            .get_bridge_update_msg(wallet.get_address_string(), execs)
            .await?;

        Ok(vec![oracle_msg, bridge_msg])
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
    Triggers {
        is_too_frequent: bool,
    },
    NoPriceFound,
}

impl PriceUpdateReason {
    fn is_too_frequent(&self) -> bool {
        match self {
            PriceUpdateReason::LastUpdateTooOld(_) => false,
            PriceUpdateReason::PriceDelta {
                is_too_frequent, ..
            } => *is_too_frequent,
            PriceUpdateReason::Triggers { is_too_frequent } => *is_too_frequent,
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
    triggers_too_frequent: u64,
    no_price_found: u64,
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
            triggers_too_frequent,
            no_price_found,
        } = self;
        write!(f, "{market} {started_tracking}: not needed {not_needed}. too old {too_old}. Delta: {delta}. Delta too frequent: {delta_too_frequent}. Triggers: {triggers}. Triggers too frequent: {triggers_too_frequent}. No price found: {no_price_found}.")
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
            triggers_too_frequent: 0,
            no_price_found: 0,
            market,
        }
    }
    fn add_reason(&mut self, reason: &Option<PriceUpdateReason>) {
        let reason = match reason {
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
            PriceUpdateReason::Triggers { is_too_frequent } => {
                if *is_too_frequent {
                    self.triggers_too_frequent += 1
                } else {
                    self.triggers += 1
                }
            }
            PriceUpdateReason::NoPriceFound => self.no_price_found += 1,
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
            PriceUpdateReason::Triggers {is_too_frequent}=> {
                write!(f, "Price would trigger positions and/or orders. Too frequent: {is_too_frequent}.")
            }
            PriceUpdateReason::NoPriceFound => write!(f, "No price point found."),
        }
    }
}
