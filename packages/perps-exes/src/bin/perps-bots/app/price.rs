use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{proto::cosmwasm::wasm::v1::MsgExecuteContract, HasAddress, TxBuilder, Wallet};
use msg::prelude::{ErrorId, PerpError};
use rust_decimal::Decimal;
use serde_json::de::StrRead;

use crate::{
    config::BotConfigByType,
    util::{
        markets::{get_markets, Market, PriceApi},
        oracle::Pyth,
    },
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};

use super::{App, AppBuilder};

#[derive(Clone)]
struct Worker {
    wallet: Arc<Wallet>,
}

/// Start the background thread to keep options pools up to date.
impl AppBuilder {
    pub(super) async fn start_price(&mut self) -> Result<()> {
        if let Some(price_wallet) = self.app.config.price_wallet.clone() {
            match &self.app.config.by_type {
                BotConfigByType::Testnet { inner } => {
                    let inner = inner.clone();
                    self.refill_gas(&inner, *price_wallet.address(), "price-bot")?;
                }
                BotConfigByType::Mainnet { inner } => {
                    self.alert_on_low_gas(
                        *price_wallet.address(),
                        "price-bot",
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
                    let (status, msg) = self
                        .get_tx_symbol(wallet, market, symbol, symbol_usd)
                        .await?;
                    statuses.push(format!("{}: {status}", market.market_id));
                    builder.add_message_mut(msg);
                }

                PriceApi::Pyth(pyth) => {
                    let msgs = self.get_txs_pyth(wallet, market, pyth).await?;
                    for msg in msgs {
                        builder.add_message_mut(msg);
                    }
                    statuses.push(format!("{}: got pyth contract messages", market.market_id));
                }
            }
        }

        let broadcast_status = match builder.sign_and_broadcast(&self.cosmos, wallet).await {
            Ok(res) => {
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

                format!("Prices updated in oracles with txhash {}", res.txhash)
            }
            Err(e) => {
                let maybe_tonic_error = e.downcast_ref::<tonic::Status>();
                let maybe_perp_error =
                    maybe_tonic_error.and_then(|status| parse_perp_error(status.message()));

                let price_exists_error = maybe_perp_error.as_ref().and_then(|perp_err| {
                    if perp_err.id == ErrorId::PriceAlreadyExists {
                        Some(perp_err)
                    } else {
                        None
                    }
                });

                let price_too_old_error = maybe_perp_error.as_ref().and_then(|perp_err| {
                    if perp_err.id == ErrorId::PriceTooOld {
                        Some(perp_err)
                    } else {
                        None
                    }
                });

                if let Some(err) = price_exists_error {
                    format!("This price point already exists: {:?}", err)
                } else if let Some(err) = price_too_old_error {
                    format!("The price is too old: {:?}", err)
                } else {
                    anyhow::bail!(
                        "is tonic error: {}, is perp error: {}, raw error: {e:?}",
                        maybe_tonic_error.is_some(),
                        maybe_perp_error.is_some()
                    );
                }
            }
        };
        statuses.push(broadcast_status);

        Ok(WatchedTaskOutput {
            skip_delay: false,
            message: statuses.join("\n"),
        })
    }

    async fn get_tx_symbol(
        &self,
        wallet: &Wallet,
        market: &Market,
        price_api_symbol: &str,
        collateral_price_api_symbol: &Option<String>,
    ) -> Result<(String, MsgExecuteContract)> {
        let price = self.get_current_price_symbol(price_api_symbol).await?;
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

// todo - this can be moved somewhere more general
fn parse_perp_error(err: &str) -> Option<PerpError<serde_json::Value>> {
    // This weird parsing to (1) strip off the content before the JSON body
    // itself and (2) ignore the trailing data after the JSON data
    let start = err.find(" {")?;
    let err = &err[start..];
    serde_json::Deserializer::new(StrRead::new(err))
        .into_iter()
        .next()?
        .ok()
}

#[cfg(test)]
mod tests {
    use msg::prelude::{ErrorDomain, ErrorId, PerpError};

    use super::*;

    #[test]
    fn test_parse_perp_error() {
        const INPUT: &str = "failed to execute message; message index: 0: {\n  \"id\": \"exceeded\",\n  \"domain\": \"faucet\",\n  \"description\": \"exceeded tap limit, wait 284911 more seconds\",\n  \"data\": {\n    \"wait_secs\": \"284911\"\n  }\n}: execute wasm contract failed [CosmWasm/wasmd@v0.29.2/x/wasm/keeper/keeper.go:425] With gas wanted: '0' and gas used: '115538' ";
        let expected = PerpError {
            id: ErrorId::Exceeded,
            domain: ErrorDomain::Faucet,
            description: "exceeded tap limit, wait 284911 more seconds".to_owned(),
            data: None,
        };
        let mut actual = parse_perp_error(INPUT).unwrap();
        actual.data = None;
        assert_eq!(actual, expected);
    }
}
