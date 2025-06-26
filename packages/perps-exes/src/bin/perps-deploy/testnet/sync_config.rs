use anyhow::{Context, Result};
use cosmwasm_std::Addr;
use perps_exes::{
    config::{ChainConfig, ConfigUpdateAndBorrowFee, MarketConfigUpdates, PriceConfig},
    contracts::{Factory, MarketInfo},
    prelude::{MarketContract, MarketId},
};
use perpswap::contracts::market::config::{Config, ConfigUpdate};

use crate::spot_price_config::get_spot_price_config;

#[derive(clap::Parser)]
pub(crate) struct SyncConfigOpts {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
    /// Which market to deposit into
    #[clap(long)]
    market: Option<MarketId>,
    /// Flag to actually execute
    #[clap(long)]
    do_it: bool,
}
impl SyncConfigOpts {
    pub(crate) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    SyncConfigOpts {
        family,
        market,
        do_it,
    }: SyncConfigOpts,
) -> Result<()> {
    let app = opt.load_app(&family).await?;
    let wallet = app.basic.get_wallet()?;
    let factory = app.tracker.get_factory(&family).await?.into_contract();

    let chain_config = ChainConfig::load(app.basic.network)?;
    let price_config = PriceConfig::load()?;
    let oracle = opt.get_oracle_info(&chain_config, &price_config, app.basic.network)?;

    let factory = Factory::from_contract(factory);

    let markets = factory.get_markets().await?;
    let markets = match market {
        Some(market) => vec![markets
            .into_iter()
            .find(|x| x.market_id == market)
            .context("Requested market not found")?],
        None => markets,
    };
    let market_config_updates = MarketConfigUpdates::load(&opt.market_config)?;

    for MarketInfo {
        market_id, market, ..
    } in markets
    {
        let market = MarketContract::new(market);
        let actual_config = market.status().await?.config;
        let ConfigUpdateAndBorrowFee {
            config: expected_config,
            initial_borrow_fee_rate: _,
        } = market_config_updates.get_market(app.basic.network, &market_id)?;
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
                // Keys that no longer exist in ConfigUpdate
            } else if is_unused_key(&key) || is_testnet_ignored_key(&key) {
                continue;
            } else {
                let expected_value = expected_config
                    .remove(&key)
                    .with_context(|| format!("Missing key in expected_config: {key}"))?;
                if expected_value.is_null() {
                    default_value
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
            if actual != expected {
                println!(
                    "Mismatched paramter for {market_id}: {key}. Actual: {}. Expected: {}.",
                    serde_json::to_string(&actual)?,
                    serde_json::to_string(&expected)?
                );
                needed_update.insert(key, expected);
            }
        }

        if needed_update.is_empty() {
            tracing::info!("No updates needed for {}", market_id);
        } else {
            tracing::info!(
                "Need to update {market_id} with:\n{}",
                serde_json::to_string_pretty(&needed_update)?
            );
            if do_it {
                let update = serde_json::Value::Object(needed_update);
                let update: ConfigUpdate = serde_json::from_value(update)?;
                let res = market.config_update(wallet, update).await?;
                tracing::info!("Updated {market_id} in {}", res.txhash);
            }
        }
    }

    Ok(())
}

/// Keys which are still in the Config struct for backwards compat but no longer in ConfigUpdate
pub(crate) fn is_unused_key(key: &str) -> bool {
    key == "limit_order_fee"
        || key == "price_update_too_old_seconds"
        || key == "staleness_seconds"
        || key == "unpend_limit"
}

/// Keys which are intentionally ignored for testnet purposes
fn is_testnet_ignored_key(key: &str) -> bool {
    key == "unstake_period_seconds"
}
