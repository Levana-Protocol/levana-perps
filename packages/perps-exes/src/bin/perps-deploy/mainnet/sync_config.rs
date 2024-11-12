use std::collections::HashSet;

use anyhow::{Context, Result};
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::{to_json_binary, Addr, CosmosMsg, Empty, WasmMsg};
use perps_exes::{
    config::{
        ChainConfig, ConfigUpdateAndBorrowFee, CrankFeeConfig, MainnetFactories,
        MarketConfigUpdates, PriceConfig,
    },
    contracts::{Factory, MarketInfo},
    prelude::MarketContract,
};
use perpswap::{
    contracts::market::{
        config::{Config, ConfigUpdate},
        entry::ExecuteOwnerMsg,
        spot_price::{SpotPriceConfig, SpotPriceFeedData},
    },
    prelude::MarketExecuteMsg,
    storage::MarketId,
};

use crate::{
    mainnet::strip_nulls, spot_price_config::get_spot_price_config,
    testnet::sync_config::is_unused_key, util::add_cosmos_msg,
};

#[derive(clap::Parser)]
pub(super) struct SyncConfigOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// Markets to sync, if empty syncs all
    #[clap(long = "market")]
    market_ids: Vec<MarketId>,
}
impl SyncConfigOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    SyncConfigOpts {
        factory,
        market_ids,
    }: SyncConfigOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let network = factory.network;

    let chain_config = ChainConfig::load(factory.network)?;
    let price_config = PriceConfig::load()?;
    let oracle = opt.get_oracle_info(&chain_config, &price_config, network)?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));
    let markets = factory.get_markets().await?;
    let markets = if market_ids.is_empty() {
        markets
    } else {
        let market = market_ids.into_iter().collect::<HashSet<_>>();
        markets
            .into_iter()
            .filter(|x| market.contains(&x.market_id))
            .collect()
    };
    let market_config_updates = MarketConfigUpdates::load(&opt.market_config)?;

    let owner = factory
        .query_owner()
        .await?
        .context("The factory owner is not provided")?;
    let mut updates = vec![];

    for MarketInfo {
        market_id, market, ..
    } in markets
    {
        let market = MarketContract::new(market);
        let actual_config = market.config().await?;
        let ConfigUpdateAndBorrowFee {
            config: expected_config,
            initial_borrow_fee_rate: _,
        } = market_config_updates
            .markets
            .get(&market_id)
            .with_context(|| format!("No market config update found for {market_id}"))?;
        let CrankFeeConfig {
            charged,
            surcharge,
            reward,
        } = market_config_updates
            .crank_fees
            .get(&network)
            .with_context(|| format!("No crank fee config found for network {network}"))?;
        let default_config = Config::new(
            perpswap::contracts::market::spot_price::SpotPriceConfig::Manual {
                admin: Addr::unchecked("ignored"),
            },
        );

        let mut actual_config = match serde_json::to_value(actual_config)? {
            serde_json::Value::Object(o) => o,
            _ => anyhow::bail!("Actual config is not an object"),
        };
        let mut expected_config = match serde_json::to_value(expected_config.clone())? {
            serde_json::Value::Object(o) => o,
            _ => anyhow::bail!("Expected config is not an object"),
        };
        let default_config = match serde_json::to_value(default_config.clone())? {
            serde_json::Value::Object(o) => o,
            _ => anyhow::bail!("Expected config is not an object"),
        };

        for key in expected_config.keys() {
            anyhow::ensure!(default_config.contains_key(key));
            anyhow::ensure!(actual_config.contains_key(key));
        }

        let mut needed_update = serde_json::Map::new();

        for (key, default_value) in default_config {
            let expected = if key == "spot_price" {
                let spot_price_config = get_spot_price_config(&oracle, &market_id)?;
                serde_json::to_value(spot_price_config)?
            } else if is_unused_key(&key) {
                continue;
            } else {
                let expected_value = expected_config
                    .remove(&key)
                    .with_context(|| format!("Missing key in expected_config: {key}"))?;
                if expected_value.is_null() {
                    if key == "crank_fee_charged" {
                        serde_json::to_value(charged)?
                    } else if key == "crank_fee_surcharge" {
                        serde_json::to_value(surcharge)?
                    } else if key == "crank_fee_reward" {
                        serde_json::to_value(reward)?
                    } else {
                        default_value
                    }
                } else {
                    if default_value == expected_value {
                        println!("Unnecessary config update {key} for market {market_id}");
                    }
                    expected_value
                }
            };
            let actual = actual_config.remove(&key).with_context(|| {
                format!("Missing actual config value {key} for market {}", market_id)
            })?;

            let matches = if key == "spot_price" {
                do_spot_prices_match_enough(actual.clone(), expected.clone())
            } else {
                actual == expected
            };
            if !matches {
                println!(
                    "Mismatched paramter for {market_id}: {key}.\nActual  : {}\nExpected: {}\n\n",
                    serde_json::to_string(&actual)?,
                    serde_json::to_string(&expected)?
                );
                needed_update.insert(key, expected);
            }
        }

        if !needed_update.is_empty() {
            let update = serde_json::Value::Object(needed_update);
            let update: ConfigUpdate = serde_json::from_value(update)?;
            updates.push(CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                contract_addr: market.get_address_string(),
                msg: to_json_binary(&strip_nulls(MarketExecuteMsg::Owner(
                    ExecuteOwnerMsg::ConfigUpdate {
                        update: Box::new(update),
                    },
                ))?)?,
                funds: vec![],
            }))
        }
    }

    if updates.is_empty() {
        println!("No updates necessary");
    } else {
        println!("CW3 contract: {owner}");
        println!("Message: {}", serde_json::to_string(&updates)?);

        let mut builder = TxBuilder::default();
        for update in &updates {
            add_cosmos_msg(&mut builder, owner, update)?;
        }
        let res = builder
            .simulate(&app.cosmos, &[owner])
            .await
            .context("Error while simulating")?;
        tracing::info!("Successfully simulated messages");
        tracing::debug!("Simulate response: {res:?}");
    }

    Ok(())
}

fn do_spot_prices_match_enough(actual: serde_json::Value, expected: serde_json::Value) -> bool {
    (|| {
        let mut actual: SpotPriceConfig = serde_json::from_value(actual)?;
        let mut expected: SpotPriceConfig = serde_json::from_value(expected)?;
        strip_unneeded(&mut actual);
        strip_unneeded(&mut expected);
        anyhow::Ok(actual == expected)
    })()
    .unwrap_or(false)
}

fn strip_unneeded(spot_price: &mut SpotPriceConfig) {
    match spot_price {
        SpotPriceConfig::Manual { admin: _ } => (),
        SpotPriceConfig::Oracle {
            pyth,
            stride,
            feeds,
            feeds_usd,
            volatile_diff_seconds: _,
        } => {
            let mut has_pyth = false;
            let mut has_stride = false;
            for feed in feeds.iter().chain(feeds_usd.iter()) {
                match feed.data {
                    SpotPriceFeedData::Constant { .. } => (),
                    SpotPriceFeedData::Pyth { .. } => has_pyth = true,
                    SpotPriceFeedData::Stride { .. } => has_stride = true,
                    SpotPriceFeedData::Sei { .. } => (),
                    SpotPriceFeedData::Simple { .. } => (),
                }
            }
            if !has_pyth {
                *pyth = None;
            }
            if !has_stride {
                *stride = None;
            }
        }
    }
}
