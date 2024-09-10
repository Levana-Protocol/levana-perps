use shared::storage::RawAddr;

use crate::prelude::*;

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary> {
    let (state, storage) = crate::types::State::load(deps, env)?;
    match msg {
        QueryMsg::Config {} => to_json_binary(&state.config),
        QueryMsg::Balance {
            address,
            start_after,
            limit,
        } => to_json_binary(&balance(
            state,
            storage,
            address,
            start_after,
            limit.map_or(10, |limit| usize::try_from(limit).unwrap()),
        )?),
        QueryMsg::Markets { start_after, limit } => to_json_binary(&markets(
            state,
            storage,
            start_after,
            limit.map_or(Ok(5), |limit| {
                usize::try_from(limit).map(|limit| limit.min(5))
            })?,
        )?),
        QueryMsg::HasWork { market } => {
            let market = state.load_market_info(storage, &market)?;
            let totals = crate::state::TOTALS
                .may_load(storage, &market.id)?
                .unwrap_or_default();
            let work = crate::work::get_work_for(storage, &state, &market, &totals)?;
            to_json_binary(&work)
        }
    }
    .map_err(anyhow::Error::from)
}

fn balance(
    state: State,
    storage: &dyn Storage,
    address: RawAddr,
    start_after: Option<MarketId>,
    limit: usize,
) -> Result<BalanceResp> {
    let address = address.validate(state.api)?;
    let mut iter = crate::state::SHARES.prefix(&address).range(
        storage,
        start_after.as_ref().map(Bound::exclusive),
        None,
        Order::Ascending,
    );
    let mut markets = vec![];
    let mut reached_end = false;
    while markets.len() <= limit {
        match iter.next() {
            None => {
                reached_end = true;
                break;
            }
            Some(res) => {
                let (market_id, shares) = res?;
                let market_info = state.load_market_info(storage, &market_id)?;
                let totals = crate::state::TOTALS
                    .may_load(storage, &market_id)?
                    .with_context(|| {
                        format!("No totals found for market with shares: {market_id}")
                    })?;
                let pos = PositionsInfo::load(&state, &market_info)?;
                markets.push(MarketBalance {
                    token: market_info.token,
                    shares,
                    collateral: totals.shares_to_collateral(shares.raw(), &pos)?,
                    pool_size: NonZero::new(totals.shares).with_context(|| {
                        format!("No shares found for pool with share entries: {market_id}")
                    })?,
                    market: market_id,
                });
            }
        }
    }
    let next_start_after = (|| {
        if reached_end {
            return None;
        };
        let last = markets.last()?;
        iter.next()?.ok();
        Some(last.market.clone())
    })();
    Ok(BalanceResp {
        markets,
        next_start_after,
    })
}

fn markets(
    state: State,
    storage: &dyn Storage,
    start_after: Option<MarketId>,
    limit: usize,
) -> Result<MarketsResp> {
    let mut iter = crate::state::TOTALS.range(
        storage,
        start_after.as_ref().map(Bound::exclusive),
        None,
        Order::Ascending,
    );
    let mut markets = vec![];
    let mut reached_end = false;
    while markets.len() <= limit {
        match iter.next() {
            None => {
                reached_end = true;
                break;
            }
            Some(res) => {
                let (market_id, totals) = res?;
                let market_info = state.load_market_info(storage, &market_id)?;
                let pos = PositionsInfo::load(&state, &market_info)?;
                let (pos, too_many_positions) = match pos {
                    PositionsInfo::TooManyPositions { to_close: _ } => (None, true),
                    PositionsInfo::NoPositions => (None, false),
                    PositionsInfo::OnePosition { pos } => (Some(*pos), false),
                };
                markets.push(MarketStatus {
                    id: market_id,
                    collateral: totals.collateral,
                    shares: totals.shares,
                    position: pos,
                    too_many_positions,
                });
            }
        }
    }
    let next_start_after = (|| {
        if reached_end {
            return None;
        };
        let last = markets.last()?;
        iter.next()?.ok();
        Some(last.id.clone())
    })();
    Ok(MarketsResp {
        markets,
        next_start_after,
    })
}
