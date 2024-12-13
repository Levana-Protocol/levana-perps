use perpswap::storage::RawAddr;

use crate::prelude::*;

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary> {
    let (state, storage) = crate::types::State::load(deps, env)?;
    match msg {
        QueryMsg::Config {} => to_json_binary(&state.config),
        QueryMsg::Balance { address } => to_json_binary(&balance(state, storage, address)?),
        QueryMsg::Status {} => to_json_binary(&markets(state, storage)?),
        QueryMsg::HasWork {} => {
            let market = state.load_market_info(storage)?;
            let totals = crate::state::TOTALS.may_load(storage)?.unwrap_or_default();
            let work = crate::work::get_work_for(storage, &state, &market, &totals)?;
            to_json_binary(&work)
        }
    }
    .map_err(anyhow::Error::from)
}

fn balance(state: State, storage: &dyn Storage, address: RawAddr) -> Result<MarketBalance> {
    let address = address.validate(state.api)?;
    let shares = crate::state::SHARES
        .may_load(storage, &address)?
        .context("SHARES store is empty")?;

    let market_info = state.load_market_info(storage)?;
    let totals = crate::state::TOTALS
        .may_load(storage)?
        .with_context(|| format!("No totals found for market with shares: {}", market_info.id))?;
    let pos = PositionsInfo::load(&state, &market_info)?;

    let result = MarketBalance {
        token: market_info.token,
        shares,
        collateral: NonZero::new(totals.shares_to_collateral(shares.raw(), &pos)?)
            .with_context(|| format!("Ended up with 0 collateral for market {}", market_info.id))?,
        pool_size: NonZero::new(totals.shares).with_context(|| {
            format!(
                "No shares found for pool with share entries: {}",
                market_info.id
            )
        })?,
        market: market_info.id,
    };
    Ok(result)
}

fn markets(state: State, storage: &dyn Storage) -> Result<MarketStatus> {
    let totals = crate::state::TOTALS
        .may_load(storage)?
        .context("TOTALS store is empty")?;
    let market_info = state.load_market_info(storage)?;
    let pos = PositionsInfo::load(&state, &market_info)?;
    let (pos, too_many_positions) = match pos {
        PositionsInfo::TooManyPositions { to_close: _ } => (None, true),
        PositionsInfo::NoPositions => (None, false),
        PositionsInfo::OnePosition { pos } => (Some(*pos), false),
    };
    let result = MarketStatus {
        id: market_info.id,
        collateral: totals.collateral,
        shares: totals.shares,
        position: pos,
        too_many_positions,
    };
    Ok(result)
}
