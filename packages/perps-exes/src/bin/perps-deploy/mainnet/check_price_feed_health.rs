use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cosmos::{Address, HasCosmos};
use perpswap::contracts::market::spot_price::{SpotPriceConfig, SpotPriceFeedData};
use perps_exes::{
    config::MainnetFactories,
    contracts::{Factory, MarketInfo},
    prelude::{MarketContract, Timestamp},
};

#[derive(clap::Parser)]
pub(super) struct CheckPriceFeedHealthOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
}
impl CheckPriceFeedHealthOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    CheckPriceFeedHealthOpts { factory }: CheckPriceFeedHealthOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));
    let markets = factory.get_markets().await?;

    for MarketInfo {
        market, market_id, ..
    } in markets
    {
        tracing::info!("Checking health of {market_id}");
        let market = MarketContract::new(market);
        match market.status().await?.config.spot_price {
            SpotPriceConfig::Manual { .. } => {
                anyhow::bail!("Unexpected manual spot price config for {market_id}")
            }
            SpotPriceConfig::Oracle {
                pyth: _,
                stride,
                feeds,
                feeds_usd,
                volatile_diff_seconds: _,
            } => {
                for feed in feeds.into_iter().chain(feeds_usd) {
                    match feed.data {
                        SpotPriceFeedData::Constant { .. } => (),
                        SpotPriceFeedData::Pyth { .. } => (),
                        SpotPriceFeedData::Stride {
                            denom,
                            age_tolerance_seconds: _,
                        } => {
                            let stride = stride.as_ref().with_context(|| {
                                format!("No stride config found for {market_id}")
                            })?;
                            let address = Address::from_str(stride.contract_address.as_str())?;
                            let stride = market.get_cosmos().make_contract(address);
                            #[derive(serde::Deserialize)]
                            struct Resp {
                                update_time: i64,
                            }
                            let Resp { update_time } = stride
                                .query(serde_json::json!({"redemption_rate":{"denom":denom}}))
                                .await?;
                            let timestamp = DateTime::from_timestamp(update_time, 0)
                                .expect("Invalid timestamp");
                            let now = Utc::now();
                            let age = now.signed_duration_since(timestamp);
                            tracing::info!("Stride contract update age: {age}");
                            if age.num_hours() > 12 {
                                tracing::error!(
                                    "{market_id} uses a stride contract with an old price update"
                                );
                            }
                        }
                        SpotPriceFeedData::Sei { .. } => (),
                        SpotPriceFeedData::Simple {
                            contract,
                            age_tolerance_seconds: _,
                        } => {
                            let address = Address::from_str(contract.as_str())?;
                            let simple = market.get_cosmos().make_contract(address);
                            #[derive(serde::Deserialize)]
                            struct Resp {
                                timestamp: Timestamp,
                            }
                            let Resp { timestamp } =
                                simple.query(serde_json::json!({"price":{}})).await?;
                            let timestamp = timestamp.try_into_chrono_datetime()?;
                            let now = Utc::now();
                            let age = now.signed_duration_since(timestamp);
                            tracing::info!("Simple contract update age: {age}");
                            if age.num_hours() > 12 {
                                tracing::error!(
                                    "{market_id} uses a simple contract with an old price update"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
