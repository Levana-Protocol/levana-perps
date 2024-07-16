use crate::prelude::*;

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary> {
    let state = crate::types::State::load(deps.api, deps.querier, deps.storage)?;
    match msg {
        QueryMsg::Config {} => {
            to_json_binary(&state.config).context("Unable to render Config to JSON")
        }
        QueryMsg::Balance {
            address,
            start_after,
            limit,
        } => {
            let address = address.validate(state.api)?;
            let mut iter = crate::state::SHARES.prefix(&address).range(
                deps.storage,
                start_after.as_ref().map(Bound::exclusive),
                None,
                Order::Ascending,
            );
            let mut markets = vec![];
            let mut next_start_after = None;
            let limit = limit.map_or(10, |limit| usize::try_from(limit).unwrap());
            while markets.len() <= limit {
                match iter.next() {
                    None => break,
                    Some(res) => {
                        let (market_id, shares) = res?;
                        let market_info = state.load_market_info(deps.storage, &market_id)?.0;
                        let totals = crate::state::TOTALS
                            .may_load(deps.storage, &market_id)?
                            .with_context(|| {
                                format!("No totals found for market with shares: {market_id}")
                            })?;
                        let pos = PositionsInfo::load();
                        markets.push(MarketBalance {
                            token: market_info.token,
                            shares,
                            collateral: NonZero::new(
                                totals.shares_to_collateral(shares.raw(), &pos)?,
                            )
                            .with_context(|| {
                                format!("Ended up with 0 collateral for market {market_id}")
                            })?,
                            pool_size: NonZero::new(totals.shares).with_context(|| {
                                format!("No shares found for pool with share entries: {market_id}")
                            })?,
                            market: market_id,
                        });
                    }
                }
            }
            to_json_binary(&BalanceResp {
                markets,
                next_start_after,
            })
            .map_err(Into::into)
        }
        QueryMsg::HasWork { market } => todo!(),
    }
}
