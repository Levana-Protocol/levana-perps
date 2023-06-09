use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use chrono::Utc;
use cosmos::{proto::cosmwasm::wasm::v1::MsgExecuteContract, HasAddress, TxBuilder, Wallet};
use msg::prelude::{PriceBaseInQuote, Signed, UnsignedDecimal};
use perps_exes::prelude::MarketContract;
use rust_decimal::Decimal;

use crate::{
    config::BotConfigByType,
    util::{
        markets::{get_markets, Market, PriceApi},
        oracle::Pyth,
    },
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};

use super::{gas_check::GasCheckWallet, App, AppBuilder};

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
impl WatchedTask for Worker {
    async fn run_single(&mut self, app: &App, _heartbeat: Heartbeat) -> Result<WatchedTaskOutput> {
        app.single_update(&self.wallet).await
    }
}

impl App {
    async fn single_update(&self, wallet: &Wallet) -> Result<WatchedTaskOutput> {
        let mut statuses = vec![];
        let mut builder = TxBuilder::default();
        let mut has_messages = false;
        let factory = self.get_factory_info().factory;
        let factory = self.cosmos.make_contract(factory);

        let markets = get_markets(&self.cosmos, &factory).await?;

        anyhow::ensure!(!markets.is_empty(), "Cannot have empty markets vec");

        for market in &markets {
            let price_apis = &market
                .get_price_api(wallet, &self.cosmos, &self.config)
                .await?;
            match price_apis {
                PriceApi::Manual { symbol, symbol_usd } => {
                    let price = self
                        .get_current_price_symbol(symbol)
                        .await?
                        .to_string()
                        .parse()?;
                    if self.needs_price_update(market, price).await? {
                        has_messages = true;
                        let (status, msg) = self
                            .get_tx_symbol(wallet, market, symbol_usd, price)
                            .await?;
                        statuses.push(format!("{}: {status}", market.market_id));
                        builder.add_message_mut(msg);
                    } else {
                        statuses.push(format!(
                            "{}: no manual price update needed",
                            market.market_id
                        ));
                    }
                }

                PriceApi::Pyth(pyth) => {
                    let (latest_price, _) = pyth.get_latest_price(&self.client).await?;
                    if self.needs_price_update(market, latest_price).await? {
                        has_messages = true;
                        let msgs = self.get_txs_pyth(wallet, market, pyth).await?;
                        for msg in msgs {
                            builder.add_message_mut(msg);
                        }
                        statuses.push(format!("{}: got pyth contract messages", market.market_id));
                    } else {
                        statuses.push(format!("{}: no pyth price update needed", market.market_id));
                    }
                }
            }
        }

        if !has_messages {
            return Ok(WatchedTaskOutput {
                skip_delay: false,
                message: statuses.join("\n"),
            });
        }

        // Take the crank lock for the rest of the execution
        let _crank_lock = self.crank_lock.lock().await;

        let res = builder.sign_and_broadcast(&self.cosmos, wallet).await?;

        // just for logging pyth prices
        for market in &markets {
            match &market
                .get_price_api(wallet, &self.cosmos, &self.config)
                .await?
            {
                PriceApi::Manual { .. } => {}

                PriceApi::Pyth(pyth) => {
                    let market_price = pyth.query_price(120).await?;
                    statuses.push(format!(
                        "{} updated pyth price: {:?}",
                        market.market_id, market_price
                    ));
                }
            }
        }

        if !res.data.is_empty() {
            statuses.push(format!("Response data from contracts: {}", res.data));
        }

        statuses.push(format!(
            "Prices updated in oracles with txhash {}",
            res.txhash
        ));

        Ok(WatchedTaskOutput {
            skip_delay: false,
            message: statuses.join("\n"),
        })
    }

    /// Does the market need a price update?
    async fn needs_price_update(
        &self,
        market: &Market,
        latest_price: PriceBaseInQuote,
    ) -> Result<bool> {
        let market = MarketContract::new(market.market.clone());
        let price = market.current_price().await?;

        // Check 1: is the last price update too old?
        let updated = price.timestamp.try_into_chrono_datetime()?;
        let age = Utc::now().signed_duration_since(updated);
        let age_secs = age.num_seconds();
        if age_secs > self.config.max_price_age_secs.into() {
            return Ok(true);
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
            return Ok(true);
        }

        // Check 3: would any triggers happen from this price?
        // We save this for last since it requires a network round trip
        market.price_would_trigger(latest_price).await
    }

    async fn get_tx_symbol(
        &self,
        wallet: &Wallet,
        market: &Market,
        collateral_price_api_symbol: &Option<String>,
        price: PriceBaseInQuote,
    ) -> Result<(String, MsgExecuteContract)> {
        let price_usd = match collateral_price_api_symbol {
            Some(symbol) if symbol.as_str() == "USDC_USD" => Some("1".parse()?),
            Some(symbol) => Some(
                self.get_current_price_symbol(symbol)
                    .await?
                    .to_string()
                    .parse()?,
            ),
            None => None,
        };

        Ok((
            format!("Updated price for {}: {}", market.market_id, price),
            MsgExecuteContract {
                sender: wallet.get_address_string(),
                contract: market.market.get_address_string(),
                msg: serde_json::to_vec(&msg::contracts::market::entry::ExecuteMsg::SetPrice {
                    price: price.to_string().parse()?,
                    price_usd,
                    execs: self.config.execs_per_price,
                    rewards: None,
                })?,
                funds: vec![],
            },
        ))
    }

    async fn get_current_price_symbol(&self, price_api_symbol: &str) -> Result<Decimal> {
        #[derive(serde::Deserialize)]
        struct Latest {
            latest_price: Decimal,
        }

        let url = match &self.config.by_type {
            BotConfigByType::Testnet { inner } => {
                format!("{}current?marketId={}", inner.price_api, price_api_symbol)
            }
            BotConfigByType::Mainnet { .. } => {
                anyhow::bail!("On mainnet, we must use Pyth price oracles")
            }
        };

        let Latest { latest_price } = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(latest_price)
    }

    async fn get_txs_pyth(
        &self,
        wallet: &Wallet,
        market: &Market,
        pyth: &Pyth,
    ) -> Result<Vec<MsgExecuteContract>> {
        let vaas = pyth.get_wormhole_proofs(&self.client).await?;
        let oracle_msg = pyth
            .get_oracle_update_msg(wallet.get_address_string(), vaas)
            .await?;
        let bridge_msg = pyth
            .get_bridge_update_msg(wallet.get_address_string(), market.market_id.clone())
            .await?;

        Ok(vec![oracle_msg, bridge_msg])
    }
}
