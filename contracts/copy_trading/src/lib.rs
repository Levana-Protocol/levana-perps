use anyhow::Context;
use cw2::{get_contract_version, set_contract_version};
use prelude::*;
use semver::Version;

mod common;
mod execute;
mod prelude;
mod query;
mod state;
mod types;
mod work;

pub use execute::execute;
pub use query::query;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:copy_trading";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    InstantiateMsg {
        factory,
        admin,
        leader,
        config:
            ConfigUpdate {
                name,
                description,
                commission_rate,
            },
    }: InstantiateMsg,
) -> Result<Response> {
    let config = Config {
        admin: admin.validate(deps.api).context("Invalid admin provided")?,
        pending_admin: None,
        factory: factory
            .validate(deps.api)
            .context("Invalid factory provided")?,
        leader: leader
            .validate(deps.api)
            .context("Invalid leader provided")?,
        name: name.unwrap_or_else(|| "Name".to_owned()),
        description: description.unwrap_or_else(|| "Description".to_owned()),
        commission_rate: commission_rate.unwrap_or_else(|| Decimal256::from_ratio(10u32, 100u32)),
        created_at: env.block.time.into(),
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

        Ok(attr_map! {
            "old_contract_name" => old_cw2.contract,
            "old_contract_version" => old_cw2.version,
            "new_contract_name" => CONTRACT_NAME,
            "new_contract_version" => CONTRACT_VERSION,
        })
    }
}
