use std::path::PathBuf;

use anyhow::{Context, Result};
use cosmos::HasAddress;
use cosmwasm_std::{to_binary, CosmosMsg, Empty, WasmMsg};
use msg::{
    contracts::market::{config::ConfigUpdate, entry::ExecuteOwnerMsg},
    prelude::MarketExecuteMsg,
};
use perps_exes::{
    config::{MainnetFactories, MarketConfigUpdates},
    contracts::{Factory, MarketInfo},
    prelude::MarketContract,
};

use crate::mainnet::strip_nulls;

#[derive(clap::Parser)]
pub(super) struct SyncConfigOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
}
impl SyncConfigOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(opt: crate::cli::Opt, SyncConfigOpts { factory }: SyncConfigOpts) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));
    let markets = factory.get_markets().await?;
    let market_config_updates = MarketConfigUpdates::load(&opt.market_config)?;

    let owner = factory.query_owner().await?;
    let mut updates = vec![];

    for MarketInfo {
        market_id, market, ..
    } in markets
    {
        let market = MarketContract::new(market);
        let actual_config = market.status().await?.config;
        let expected_config = market_config_updates
            .markets
            .get(&market_id)
            .with_context(|| format!("No market config update found for {market_id}"))?;

        let mut actual_config = match serde_json::to_value(actual_config)? {
            serde_json::Value::Object(o) => o,
            _ => anyhow::bail!("Actual config is not an object"),
        };
        let expected_config = match serde_json::to_value(expected_config.clone())? {
            serde_json::Value::Object(o) => o,
            _ => anyhow::bail!("Expected config is not an object"),
        };

        let mut needed_update = serde_json::Map::new();

        for (key, expected) in expected_config {
            if expected.is_null() {
                continue;
            }
            let actual = actual_config.remove(&key).with_context(|| {
                format!("Missing actual config value {key} for market {}", market_id)
            })?;
            if actual != expected {
                println!(
                    "Mismatched paramter for {market_id}: {key}. Actual: {}. Expected: {}.",
                    serde_json::to_string(&actual)?,
                    serde_json::to_string(&expected)?
                );
            }
            needed_update.insert(key, expected);
        }

        if !needed_update.is_empty() {
            let update = serde_json::Value::Object(needed_update);
            let update: ConfigUpdate = serde_json::from_value(update)?;
            updates.push(CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                contract_addr: market.get_address_string(),
                msg: to_binary(&strip_nulls(MarketExecuteMsg::Owner(
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
    }

    Ok(())
}
