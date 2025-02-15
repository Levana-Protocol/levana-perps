mod common;
mod execute;
mod prelude;
mod query;
mod reply;
mod state;
mod types;
mod work;

use std::str::FromStr;

use cw2::{get_contract_version, set_contract_version};
use perpswap::storage::LeverageToBase;
use prelude::*;
use semver::Version;

pub use execute::execute;
pub use query::query;
pub use reply::reply;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:countertrade";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    InstantiateMsg {
        market,
        admin,
        config:
            ConfigUpdate {
                min_funding,
                target_funding,
                max_funding,
                max_leverage,
                iterations,
                take_profit_factor,
                stop_loss_factor,
            },
    }: InstantiateMsg,
) -> Result<Response> {
    let config = Config {
        admin: admin.validate(deps.api).context("Invalid admin provided")?,
        pending_admin: None,
        market: market
            .validate(deps.api)
            .context("Invalid market provided")?,
        min_funding: min_funding.unwrap_or_else(|| Decimal256::from_ratio(10u32, 100u32)),
        target_funding: target_funding.unwrap_or_else(|| Decimal256::from_ratio(40u32, 100u32)),
        max_funding: max_funding.unwrap_or_else(|| Decimal256::from_ratio(60u32, 100u32)),
        max_leverage: max_leverage.unwrap_or_else(|| LeverageToBase::from_str("10").unwrap()),
        iterations: iterations.unwrap_or(50),
        take_profit_factor: take_profit_factor
            .unwrap_or_else(|| Decimal256::from_ratio(15u32, 10u32)),
        stop_loss_factor: stop_loss_factor.unwrap_or_else(Decimal256::one),
    };
    config.check()?;
    state::CONFIG
        .save(deps.storage, &config)
        .context("Cannot save config")?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)
        .context("Cannot set contract version")?;

    Ok(Response::new())
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, MigrateMsg {}: MigrateMsg) -> Result<Response> {
    let old_cw2 = get_contract_version(deps.storage).context("Could not load contract version")?;
    let old_version: Version = old_cw2
        .version
        .parse()
        .context("Couldn't parse old contract version")?;
    let new_version: Version = CONTRACT_VERSION
        .parse()
        .context("Couldn't parse new contract version")?;

    if old_cw2.contract != CONTRACT_NAME {
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
    }
}
