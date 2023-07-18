use std::{fmt::Display, sync::Arc};

use anyhow::Result;
use axum::async_trait;
use chrono::{Duration, Utc};
use cosmos::{proto::cosmwasm::wasm::v1::MsgExecuteContract, HasAddress, TxBuilder, Wallet};
use cosmwasm_std::Decimal256;
use msg::prelude::{PriceBaseInQuote, PriceCollateralInUsd, Signed, UnsignedDecimal};
use perps_exes::pyth::{get_latest_price, get_oracle_update_msg};

use crate::{
    config::BotConfigByType,
    util::{
        markets::{Market, PriceApi},
        oracle::Pyth,
    },
    watcher::{WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, gas_check::GasCheckWallet, App, AppBuilder};

#[derive(Clone)]
struct Worker {
    wallet: Arc<Wallet>,
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
        let message = app.single_update(&self.wallet, market).await?;
        Ok(WatchedTaskOutput {
            skip_delay: false,
            message,
        })
    }
}

impl App {
    async fn single_update(&self, wallet: &Wallet, market: &Market) -> Result<String> {
        let mut statuses = vec![];
        let mut builder = TxBuilder::default();

        let price_apis = &market
            .get_price_api(wallet, &self.cosmos, &self.pyth_config)
            .await?;
        let pyth_opt = match price_apis {
            PriceApi::Manual(feeds) => {
                let (price, price_usd) =
                    get_latest_price(&self.client, feeds, &self.endpoints).await?;

                if let Some(reason) = self.needs_price_update(market, price).await? {
                    let (status, msg) = self.get_tx_manual(wallet, market, price, price_usd)?;
                    builder.add_message_mut(msg);
                    statuses.push(format!("Needs manual update: {reason} {status}"));
                    None
                } else {
                    return Ok("No manual price update needed".to_owned());
                }
            }

            PriceApi::Pyth(pyth) => {
                let (latest_price, _) =
                    get_latest_price(&self.client, &pyth.market_price_feeds, &self.endpoints)
                        .await?;
                if let Some(reason) = self.needs_price_update(market, latest_price).await? {
                    let msgs = self.get_txs_pyth(wallet, market, pyth).await?;
                    for msg in msgs {
                        builder.add_message_mut(msg);
                    }
                    statuses.push(format!("Needs Pyth update: {reason}"));
                    Some(pyth)
                } else {
                    return Ok("No pyth price update needed".to_owned());
                }
            }
        };

        // Take the crank lock for the rest of the execution
        let crank_lock = self.crank_lock.lock().await;

        let res = match builder.sign_and_broadcast(&self.cosmos, wallet).await {
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
                let current_price = market.market.current_price().await?;
                let last_update = current_price.timestamp.try_into_chrono_datetime()?;
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

        std::mem::drop(crank_lock);

        // just for logging pyth prices
        if let Some(pyth) = pyth_opt {
            let msg = match pyth.query_price(120).await {
                Ok(market_price) => format!("Updated pyth price: {market_price:?}"),
                Err(e) => format!("query_price failed, ignoring: {e:?}."),
            };
            statuses.push(msg);
        }

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
        latest_price: PriceBaseInQuote,
    ) -> Result<Option<PriceUpdateReason>> {
        let market = &market.market;
        let price = market.current_price().await;

        let price = match price {
            Ok(price) => price,
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

        // Check 1: is the last price update too old?
        let updated = price.timestamp.try_into_chrono_datetime()?;
        let age = Utc::now().signed_duration_since(updated);
        let age_secs = age.num_seconds();
        if age_secs > self.config.max_price_age_secs.into() {
            return Ok(Some(PriceUpdateReason::LastUpdateTooOld(age)));
        }

        // Check 2: has the price moved more than the allowed delta?
        let delta = latest_price
            .into_non_zero()
            .raw()
            .checked_div(price.price_base.into_non_zero().raw())?
            .into_signed()
            .checked_sub(Signed::ONE)?
            .abs_unsigned();
        if delta >= self.config.max_allowed_price_delta {
            return Ok(Some(PriceUpdateReason::PriceDelta {
                old: price.price_base,
                new: latest_price,
                delta,
            }));
        }

        // Check 3: would any triggers happen from this price?
        // We save this for last since it requires a network round trip
        if market.price_would_trigger(latest_price).await? {
            return Ok(Some(PriceUpdateReason::Triggers));
        }

        Ok(None)
    }

    fn get_tx_manual(
        &self,
        wallet: &Wallet,
        market: &Market,
        price: PriceBaseInQuote,
        price_usd: Option<PriceCollateralInUsd>,
    ) -> Result<(String, MsgExecuteContract)> {
        Ok((
            format!("Updated price for {}: {}", market.market_id, price),
            MsgExecuteContract {
                sender: wallet.get_address_string(),
                contract: market.market.get_address_string(),
                msg: serde_json::to_vec(&msg::contracts::market::entry::ExecuteMsg::SetPrice {
                    price,
                    price_usd,
                    execs: self.config.execs_per_price,
                    rewards: None,
                })?,
                funds: vec![],
            },
        ))
    }

    async fn get_txs_pyth(
        &self,
        wallet: &Wallet,
        market: &Market,
        pyth: &Pyth,
    ) -> Result<Vec<MsgExecuteContract>> {
        let oracle_msg = get_oracle_update_msg(
            &pyth.market_price_feeds,
            &wallet,
            &self.endpoints,
            &self.client,
            &pyth.oracle,
        )
        .await?;
        let bridge_msg = pyth
            .get_bridge_update_msg(wallet.get_address_string(), market.market_id.clone())
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
    },
    Triggers,
    NoPriceFound,
}

impl Display for PriceUpdateReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PriceUpdateReason::LastUpdateTooOld(age) => write!(f, "Last update too old: {age}."),
            PriceUpdateReason::PriceDelta { old, new, delta } => write!(
                f,
                "Large price delta. Old: {old}. New: {new}. Delta: {delta}."
            ),
            PriceUpdateReason::Triggers => {
                write!(f, "Price would trigger positions and/or orders.")
            }
            PriceUpdateReason::NoPriceFound => write!(f, "No price point found."),
        }
    }
}
