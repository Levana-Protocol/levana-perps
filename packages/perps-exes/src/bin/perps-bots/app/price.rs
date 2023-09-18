use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    sync::Arc,
};

use anyhow::Result;
use axum::async_trait;
use chrono::{DateTime, Duration, Utc};
use cosmos::{
    proto::{cosmos::base::abci::v1beta1::TxResponse, cosmwasm::wasm::v1::MsgExecuteContract},
    TxBuilder, Wallet,
};
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
    async fn run_multi_market(
        &mut self,
        app: &App,
        _factory: &FactoryInfo,
        markets: &[Market],
    ) -> Result<WatchedTaskOutput> {
        let (messages, tx_res) = app.multi_update(markets, self).await?;

        let mut combined_message = String::new();

        for (market_id, message) in messages.iter() {
            let stats = self
                .stats
                .entry(market_id.clone())
                .or_insert_with(|| ReasonStats::new(market_id.clone()));
            log::info!("Price update for {}: {}", market_id, message);
            combined_message.push_str(&format!("{message}. {stats}\n"));
        }

        if let Some(res) = tx_res {
            if !res.data.is_empty() {
                combined_message.push_str(&format!("Response data from contracts: {}", res.data));
            }

            combined_message.push_str(&format!(
                "Prices updated in oracles with txhash {}",
                res.txhash
            ));
        }

        Ok(WatchedTaskOutput {
            skip_delay: false,
            message: combined_message,
        })
    }
}

impl App {
    async fn multi_update(
        &self,
        markets: &[Market],
        worker: &mut Worker,
    ) -> Result<(HashMap<MarketId, String>, Option<TxResponse>)> {
        // Take the crank lock for the duration of this execution
        let _crank_lock = self.crank_lock.lock().await;

        let mut builder = TxBuilder::default();
        let mut results = HashMap::new();
        let mut market_prices = HashMap::new();
        let mut pyth_markets = HashSet::new();

        let mut has_tx = false;

        for market in markets {
            // Load it up each time in case there are config changes, but we could
            // theoretically optimize this by doing it at load time instead.
            let oracle = Oracle::new(
                &self.cosmos,
                market,
                self.endpoints_stable.clone(),
                self.endpoints_edge.clone(),
            )
            .await?;

            let (oracle_price, _) = oracle.get_latest_price(&self.client).await?;

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

            if let Some(reason) = reason {
                if reason.is_too_frequent() {
                    results.insert(
                        market.market_id.clone(),
                        "Too frequent price updates, skipping".to_owned(),
                    );
                    continue;
                } else {
                    results.insert(
                        market.market_id.clone(),
                        format!("Needs price update: {reason}"),
                    );
                    has_tx = true;
                }
            } else {
                results.insert(
                    market.market_id.clone(),
                    "No pyth price update needed".to_owned(),
                );
                continue;
            }

            let pyth_msg = self.get_tx_pyth(&worker.wallet, &oracle).await?;
            if let Some(msg) = pyth_msg {
                builder.add_message_mut(msg);
                pyth_markets.insert(market.market_id.clone());
            }

            builder.add_message_mut(market.market.get_crank_msg(&worker.wallet, Some(1))?);

            market_prices.insert(market.market_id.clone(), market_price);
        }

        if !has_tx {
            return Ok((results, None));
        }

        let res = match builder
            .sign_and_broadcast(&self.cosmos, &worker.wallet)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                let mut all_errors_ok = true;
                // PERP-1702: If the price is too old, only complain after a
                // longer period of time to avoid spurious alerts.

                // Hacky way to check if we're getting this error, we could
                // parse the error correctly, but this is Good Enough.
                if !format!("{e:?}").contains("price_too_old") {
                    return Err(e);
                }

                for (market_id, market_price) in market_prices.iter() {
                    // OK, it was a too old error. Let's find out when the last price update was for the contract.
                    if let Some(prev_publish_time) =
                        market_price.and_then(|price| price.publish_time)
                    {
                        let last_update = prev_publish_time.try_into_chrono_datetime()?;
                        let now = Utc::now();
                        let age = now - last_update;

                        if u32::try_from(age.num_seconds())?
                            > self.config.price_age_alert_threshold_secs
                        {
                            all_errors_ok = false;
                            results.insert(market_id.clone(), format!("{market_id}: {:?}", e));
                        } else {
                            results.insert(market_id.clone(), format!(
                                "Ignoring failed price update. Price age in contract for {market_id} is: {age}"
                            ));
                        }
                    } else {
                        // no publish time, so we can't tell how old the price is
                        all_errors_ok = false;
                        results.insert(market_id.clone(), format!("{market_id}: {:?}", e));
                    }
                }

                if !all_errors_ok {
                    bail!("{:#?}", results);
                } else {
                    return Ok((results, None));
                }
            }
        };

        for market in markets {
            // the market must have been updated from the above transaction
            let updated_price = market.market.current_price().await?;
            match updated_price.publish_time {
                Some(publish_time) => {
                    let timestamp = publish_time.try_into_chrono_datetime()?;
                    worker
                        .last_successful_price_publish_times
                        .insert(market.market_id.clone(), timestamp);
                }
                None => {
                    if pyth_markets.contains(&market.market_id) {
                        log::error!("No publish time for {}, but it must exist in a pyth-based price update", market.market_id);
                    }
                }
            }

            let market_price = market_prices
                .get(&market.market_id)
                .context("no in-memory market price")?;
            results
                .entry(market.market_id.clone())
                .and_modify(|x| x.push_str(&format!("Updated price: {:?}", market_price)));
        }

        Ok((results, Some(res)))
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
            return Ok((
                Some(market_price),
                Some(PriceUpdateReason::Triggers { is_too_frequent }),
            ));
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
                            &pyth.endpoints,
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
