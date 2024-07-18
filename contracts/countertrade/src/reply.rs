use crate::prelude::*;

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let (state, storage) = State::load_mut(deps, env)?;
    let reply = crate::state::REPLY
        .may_load(storage)?
        .context("In reply, but there's no reply state")?;
    crate::state::REPLY.remove(storage);
    match reply {
        ReplyState::ClosingPositions {
            market,
            previous_balance,
        } => update_collateral(state, storage, market, previous_balance),
    }
}

fn update_collateral(
    state: State,
    storage: &mut dyn Storage,
    market: MarketId,
    previous_balance: Collateral,
) -> Result<Response> {
    let mut totals = crate::state::TOTALS
        .may_load(storage, &market)?
        .with_context(|| format!("When updating collateral, no totals found for {market}"))?;
    let market = state.load_cache_market_info(storage, &market)?;
    let new_balance = state.get_local_token_balance(&market.token)?;
    let additional = new_balance
        .checked_sub(previous_balance)
        .context("Impossible balance update, new balance is less than previous balance")?;
    totals.collateral = totals.collateral.checked_add(additional)?;
    crate::state::TOTALS.save(storage, &market.id, &totals)?;
    Ok(Response::new().add_event(
        Event::new("collateral-in-reply")
            .add_attribute("additional", additional.to_string())
            .add_attribute("market", market.id.as_str()),
    ))
}
