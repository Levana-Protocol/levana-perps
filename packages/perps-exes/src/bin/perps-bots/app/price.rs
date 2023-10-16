use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    sync::Arc,
};

use anyhow::Result;
use axum::async_trait;
use chrono::{DateTime, Duration, Utc};
use cosmos::{proto::cosmwasm::wasm::v1::MsgExecuteContract, TxBuilder, Wallet};
use cosmwasm_std::Decimal256;
use msg::{
    contracts::market::spot_price::{SpotPriceConfig, SpotPriceFeedData},
    prelude::*,
};
use perps_exes::pyth::get_oracle_update_msg;
use shared::storage::MarketId;

use crate::{
    config::BotConfigByType,
    util::{markets::Market, oracle::Oracle},
    watcher::{WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, gas_check::GasCheckWallet, App, AppBuilder};

struct Worker {
    wallet: Arc<Wallet>,
    stats: HashMap<MarketId, ReasonStats>,
    last_successful_price_publish_times: HashMap<MarketId, DateTime<Utc>>,
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
                    last_successful_price_publish_times: HashMap::new(),
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
        let oracle = Oracle::new(
            &self.cosmos,
            market.clone(),
            &self.endpoint_stable,
            &self.endpoint_edge,
        )
        .await?;

        let start_time = Utc::now();
        let (oracle_price, _) = oracle.get_latest_price(&self.client).await?;
        let time_spent = Utc::now() - start_time;
        log::info!("get_latest_price took {time_spent}");

        let start_time = Utc::now();
        let (market_price, reason) = self
            .needs_price_update(
                market,
                oracle_price,
                worker
                    .last_successful_price_publish_times
                    .get(&market.market_id)
                    .copied(),
            )
            .await?;
        worker.add_reason(&market.market_id, &reason);
        let time_spent = Utc::now() - start_time;
        log::debug!("needs_price_update took {time_spent}");

        if let Some(reason) = reason {
            if reason.is_too_frequent() {
                return Ok("Too frequent price updates, skipping".to_owned());
            }

            statuses.push(format!("Needs price update: {reason}"));
        } else {
            return Ok("No price update needed".to_owned());
        }

        let start_time = Utc::now();
        let pyth_msg = self.get_tx_pyth(&worker.wallet, &oracle).await?;
        let is_pyth = pyth_msg.is_some();
        if let Some(msg) = &pyth_msg {
            builder.add_message_mut(msg.clone());
        }
        let time_spent = Utc::now() - start_time;
        log::debug!("get_tx_pyth took {time_spent}");

        builder.add_message_mut(market.market.get_crank_msg(
            &worker.wallet,
            Some(1),
            self.config.get_crank_rewards_wallet(),
        )?);

        let start_time = Utc::now();
        let res = builder
            .sign_and_broadcast(&self.cosmos, &worker.wallet)
            .await;
        let time_spent = Utc::now() - start_time;
        log::debug!("sign_and_broadcast took {time_spent}");
        let res = match res {
            Ok(res) => res,
            Err(e) => {
                // PERP-1702: If the price is too old, only complain after a
                // longer period of time to avoid spurious alerts.

                // Hacky way to check if we're getting this error, we could
                // parse the error correctly, but this is Good Enough.
                if !format!("{e:?}").contains("price_too_old") {
                    match &pyth_msg {
                        None => log::error!("price_too_old occurred with no pyth_msg: {e:?}"),
                        Some(pyth_msg) => match std::str::from_utf8(&pyth_msg.msg) {
                            Ok(msg) => log::error!("price_too_old occurred with execute message {msg}, error was {e:?}"),
                            Err(_) => log::error!("price_too_old occurred with execute message {:?}, error was {e:?}", pyth_msg.msg),
                        },
                    }
                    return Err(e);
                }

                // OK, it was a too old error. Let's find out when the last price update was for the contract.
                if let Some(prev_publish_time) = market_price.and_then(|price| price.publish_time) {
                    let last_update = prev_publish_time.try_into_chrono_datetime()?;
                    let now = Utc::now();
                    let age = now - last_update;
                    if u32::try_from(age.num_seconds())?
                        > self.config.price_age_alert_threshold_secs
                    {
                        return Err(e);
                    } else {
                        return Ok(format!(
                            "Ignoring failed price update. Price age in contract is: {age}"
                        ));
                    }
                } else {
                    // no publish time, so we can't tell how old the price is
                    return Err(e);
                }
            }
        };

        let start_time = Utc::now();
        // the market must have been updated from the above transaction
        let updated_price = market.market.current_price().await?;
        let time_spent = Utc::now() - start_time;
        log::debug!("current_price took {time_spent}");
        match updated_price.publish_time {
            Some(publish_time) => {
                let timestamp = publish_time.try_into_chrono_datetime()?;
                worker
                    .last_successful_price_publish_times
                    .insert(market.market_id.clone(), timestamp);
            }
            None => {
                if is_pyth {
                    log::error!("No publish time, but it must exist in a pyth-based price update");
                }
            }
        }

        statuses.push(format!("Updated price: {market_price:?}"));

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
        oracle_price: PriceBaseInQuote,
        last_successful_price_publish_time: Option<DateTime<Utc>>,
    ) -> Result<(Option<PricePoint>, Option<PriceUpdateReason>)> {
        let market = &market.market;
        let market_price: PricePoint = match market.current_price().await {
            Ok(price) => price,
            Err(e) => {
                let msg = format!("{e}");
                return if msg.contains("price_not_found") {
                    // Assume this is the first price being set
                    Ok((None, Some(PriceUpdateReason::NoPriceFound)))
                } else {
                    Err(e)
                };
            }
        };

        let mut is_too_frequent = false;

        if let Some(publish_time) = market_price.publish_time {
            // Determine the logical "last update" by using both the
            // contract-derived price time and the most recent successfully price
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
            if age_secs > self.config.max_price_age_secs.into() {
                return Ok((
                    Some(market_price),
                    Some(PriceUpdateReason::LastUpdateTooOld(age)),
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
                Some(PriceUpdateReason::PriceDelta {
                    old: market_price.price_base,
                    new: oracle_price,
                    delta,
                    is_too_frequent,
                }),
            ));
        }

        // Check 3: would any triggers happen from this price?
        // We save this for last since it requires a network round trip
        if market.price_would_trigger(oracle_price).await? {
            return Ok((Some(market_price), Some(PriceUpdateReason::Triggers)));
        }

        Ok((Some(market_price), None))
    }

    async fn get_tx_pyth(
        &self,
        wallet: &Wallet,
        oracle: &Oracle,
    ) -> Result<Option<MsgExecuteContract>> {
        match &oracle.pyth {
            None => Ok(None),
            Some(pyth) => {
                let mut unique_pyth_ids = HashSet::new();
                if let SpotPriceConfig::Oracle {
                    feeds, feeds_usd, ..
                } = &oracle.spot_price_config
                {
                    for feed in feeds.iter().chain(feeds_usd.iter()) {
                        if let SpotPriceFeedData::Pyth { id, .. } = feed.data {
                            unique_pyth_ids.insert(id);
                        }
                    }
                }

                match unique_pyth_ids.is_empty() {
                    true => Ok(None),
                    false => {
                        let unique_pyth_ids = unique_pyth_ids.into_iter().collect::<Vec<_>>();

                        let msg = get_oracle_update_msg(
                            &unique_pyth_ids,
                            &wallet,
                            &pyth.endpoint,
                            &self.client,
                            &pyth.contract,
                        )
                        .await?;

                        Ok(Some(msg))
                    }
                }
            }
        }
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
        } = self;
        write!(f, "{market} {started_tracking}: not needed {not_needed}. too old {too_old}. Delta: {delta}. Delta too frequent: {delta_too_frequent}. Triggers: {triggers}. No price found: {no_price_found}.")
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
            PriceUpdateReason::Triggers => self.triggers += 1,
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
            PriceUpdateReason::Triggers => {
                write!(f, "Price would trigger positions and/or orders.")
            }
            PriceUpdateReason::NoPriceFound => write!(f, "No price point found."),
        }
    }
}
