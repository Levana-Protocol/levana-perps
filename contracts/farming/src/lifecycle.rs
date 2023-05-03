use crate::prelude::*;
use semver::Version;

// version info for migration info
const CONTRACT_NAME: &str = "levana.finance:farming";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    InstantiateMsg {
        // FIXME remove the ..
        factory,
        market_id,
        ..
    }: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let factory = factory.validate(deps.api)?;
    MarketInfo::save(deps.querier, deps.storage, factory, market_id)?;

    let (_state, mut ctx) = StateContext::new(deps, env)?;
    ctx.response.add_event(NewFarming {});

    Ok(ctx.response.into_response())
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, MigrateMsg {}: MigrateMsg) -> Result<Response> {
    let old_cw2 = get_contract_version(deps.storage)?;
    let old_version: Version = old_cw2
        .version
        .parse()
        .map_err(|_| anyhow!("couldn't parse old contract version"))?;
    let new_version: Version = CONTRACT_VERSION
        .parse()
        .map_err(|_| anyhow!("couldn't parse new contract version"))?;

    if old_cw2.contract != CONTRACT_NAME {
        Err(anyhow!(
            "mismatched contract migration name (from {} to {})",
            old_cw2.contract,
            CONTRACT_NAME
        ))
    } else if old_version > new_version {
        Err(anyhow!(
            "cannot migrate contract from newer to older (from {} to {})",
            old_cw2.version,
            CONTRACT_VERSION
        ))
    } else {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        Ok(attr_map! {
            "old_contract_name" => old_cw2.contract,
            "old_contract_version" => old_cw2.version,
            "new_contract_name" => CONTRACT_NAME,
            "new_contract_version" => CONTRACT_VERSION,
        })
    }
}
