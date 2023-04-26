use std::sync::Arc;

use anyhow::Result;
use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Cosmos, HasAddress, TxBuilder, Wallet,
};
use parking_lot::RwLock;
use perps_exes::config::DeploymentConfig;
use rust_decimal::Decimal;
use tokio::sync::Mutex;

use crate::util::markets::{get_markets, Market};

use super::{
    factory::FactoryInfo,
    status_collector::{Status, StatusCategory, StatusCollector},
};

struct Worker {
    wallet: Arc<Wallet>,
    cosmos: Cosmos,
    client: reqwest::Client,
    config: Arc<DeploymentConfig>,
    factory: Arc<RwLock<Arc<FactoryInfo>>>,
}

/// Start the background thread to keep options pools up to date.
impl StatusCollector {
    pub(super) async fn start_price(
        &self,
        cosmos: Cosmos,
        client: reqwest::Client,
        config: Arc<DeploymentConfig>,
        factory: Arc<RwLock<Arc<FactoryInfo>>>,
        wallet: Arc<Wallet>,
        gas_wallet: Arc<Mutex<Wallet>>,
    ) -> Result<()> {
        self.track_gas_funds(
            *wallet.address(),
            "oracle-bot",
            config.min_gas.price,
            gas_wallet,
        );

        let worker = Arc::new(Worker {
            wallet,
            cosmos,
            client,
            config,
            factory,
        });

        self.add_status_checks(StatusCategory::Price, UPDATE_DELAY_SECONDS, move || {
            worker.clone().single_update()
        });

        Ok(())
    }
}

const UPDATE_DELAY_SECONDS: u64 = 60;
const TOO_OLD_SECONDS: i64 = 180;

impl Worker {
    async fn single_update(self: Arc<Self>) -> Vec<(String, Status)> {
        let mut statuses = vec![];
        let mut builder = TxBuilder::default();
        let mut has_tx = false;
        let factory = self.factory.read().factory;

        let (markets, status) = match get_markets(&self.cosmos, factory).await {
            Ok(markets) => {
                let status = Status::success(format!("Loaded markets: {markets:?}"), None);
                (markets, status)
            }
            Err(e) => (
                vec![],
                Status::error(format!("Unable to load markets: {e:?}")),
            ),
        };
        statuses.push(("load-markets".to_owned(), status));

        for market in markets {
            statuses.push((
                format!("market-{}", market.market_id),
                match self.get_tx(&market).await {
                    Err(e) => Status::error(format!("{e:?}")),
                    Ok((status, msg)) => {
                        has_tx = true;
                        builder.add_message_mut(msg);
                        Status::success(status, Some(TOO_OLD_SECONDS))
                    }
                },
            ));
        }

        statuses.push((
            "broadcast".to_owned(),
            if has_tx {
                match builder.sign_and_broadcast(&self.cosmos, &self.wallet).await {
                    Ok(res) => Status::success(
                        format!("Prices updated in oracles with txhash {}", res.txhash),
                        Some(TOO_OLD_SECONDS),
                    ),
                    Err(e) => Status::error(format!("{e:?}")),
                }
            } else {
                Status::error("No updated prices")
            },
        ));

        statuses
    }

    async fn get_tx(&self, market: &Market) -> Result<(String, MsgExecuteContract)> {
        let price = self.get_current_price(&market.price_api_symbol).await?;
        let price_usd = match &market.collateral_price_api_symbol {
            Some(symbol) if symbol.as_str() == "USDC_USD" => Some("1".parse()?),
            Some(symbol) => Some(self.get_current_price(symbol).await?.to_string().parse()?),
            None => None,
        };

        Ok((
            format!("Updated price for {}: {}", market.market_id, price),
            MsgExecuteContract {
                sender: self.wallet.get_address_string(),
                contract: market.market.get_address_string(),
                msg: serde_json::to_vec(&msg::contracts::market::entry::ExecuteMsg::SetPrice {
                    price: price.to_string().parse()?,
                    price_usd,
                    execs: None,
                    rewards: None,
                })?,
                funds: vec![],
            },
        ))
    }

    async fn get_current_price(&self, price_api_symbol: &str) -> Result<Decimal> {
        #[derive(serde::Deserialize)]
        struct Latest {
            latest_price: Decimal,
        }

        let url = format!(
            "{}current?marketId={}",
            self.config.price_api, price_api_symbol
        );
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
}
