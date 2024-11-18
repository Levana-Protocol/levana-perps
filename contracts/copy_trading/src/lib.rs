use anyhow::Context;
use cw2::{get_contract_version, set_contract_version};
use prelude::*;
use semver::Version;

mod common;
mod execute;
mod prelude;
mod query;
mod reply;
#[cfg(debug_assertions)]
mod sanity;
mod state;
mod types;
mod work;

pub use execute::execute;
pub use query::query;
pub use reply::reply;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:copy_trading";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    InstantiateMsg {
        leader,
        config:
            ConfigUpdate {
                name,
                description,
                commission_rate,
            },
        parameters:
            FactoryConfigUpdate {
                allowed_rebalance_queries,
                allowed_lp_token_queries,
            },
    }: InstantiateMsg,
) -> Result<Response> {
    // Sender is the factory contract
    let factory = info.sender;
    let config = Config {
        admin: factory.clone(),
        pending_admin: None,
        factory,
        leader: leader
            .validate(deps.api)
            .context("Invalid leader provided")?,
        name: name.unwrap_or_else(|| "Name".to_owned()),
        description: description.unwrap_or_else(|| "Description".to_owned()),
        commission_rate: commission_rate.unwrap_or_else(|| Decimal256::from_ratio(10u32, 100u32)),
        created_at: env.block.time.into(),
        allowed_rebalance_queries: allowed_rebalance_queries.unwrap_or(30),
        allowed_lp_token_queries: allowed_lp_token_queries.unwrap_or(30),
    };
    config.check()?;
    state::CONFIG
        .save(deps.storage, &config)
        .context("Cannot save config")?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)
        .context("Cannot set contract version")?;
    sanity(deps.storage, &env);

    Ok(Response::new())
}

#[entry_point]
pub fn migrate(deps: DepsMut, env: Env, MigrateMsg {}: MigrateMsg) -> Result<Response> {
    let old_cw2 = get_contract_version(deps.storage).context("Could not load contract version")?;
    let old_version: Version = old_cw2
        .version
        .parse()
        .context("Couldn't parse old contract version")?;
    let new_version: Version = CONTRACT_VERSION
        .parse()
        .context("Couldn't parse new contract version")?;

    let response = if old_cw2.contract != CONTRACT_NAME {
        Err(anyhow!(
            "Mismatched contract migration name (from {} to {})",
            old_cw2.contract,
            CONTRACT_NAME
        ))
    } else if old_version > new_version {
        Err(anyhow!(
            "Cannot migrate contract from newer to older (from {} to {})",
            old_cw2.version,
            CONTRACT_VERSION
        ))
    } else {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)
            .context("Could not set contract version during migration")?;
        let response = Response::new()
            .add_attribute("old_contract_name", old_cw2.contract)
            .add_attribute("old_contract_version", old_cw2.version)
            .add_attribute("new_contract_name", CONTRACT_NAME)
            .add_attribute("new_contract_version", CONTRACT_VERSION);
        Ok(response)
    };
    sanity(deps.storage, &env);
    response
}
