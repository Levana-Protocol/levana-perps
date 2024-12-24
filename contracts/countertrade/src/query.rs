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
    let shares = crate::state::SHARES.may_load(storage, &address)?;

    let market_info = state.load_market_info(storage)?;
    let totals = crate::state::TOTALS.may_load(storage)?;

    let pool_size = match totals {
        Some(ref totals) => totals.shares,
        None => LpToken::zero(),
    };

    let shares = match shares {
        Some(shares) => shares.raw(),
        None => LpToken::zero(),
    };

    let collateral = match totals {
        Some(totals) => {
            if totals.shares.is_zero() {
                Collateral::zero()
            } else {
                let contract_balance = state.contract_balance(storage)?;
                let pos = PositionsInfo::load(&state, &market_info)?;
                totals.shares_to_collateral(contract_balance, shares, &pos)?
            }
        }
        None => Collateral::zero(),
    };

    let result = MarketBalance {
        token: market_info.token,
        shares,
        collateral,
        pool_size,
        market: market_info.id,
    };
    Ok(result)
}

fn markets(state: State, storage: &dyn Storage) -> Result<MarketStatus> {
    let totals = crate::state::TOTALS.may_load(storage)?.unwrap_or_default();
    let market_info = state.load_market_info(storage)?;
    let pos = PositionsInfo::load(&state, &market_info)?;
    let (pos, too_many_positions) = match pos {
        PositionsInfo::TooManyPositions { to_close: _ } => (None, true),
        PositionsInfo::NoPositions => (None, false),
        PositionsInfo::OnePosition { pos } => (Some(*pos), false),
    };
    let contract_balance = state.contract_balance(storage)?;
    let result = MarketStatus {
        id: market_info.id,
        collateral: contract_balance,
        shares: totals.shares,
        position: pos,
        too_many_positions,
    };
    Ok(result)
}
