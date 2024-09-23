use crate::{
    prelude::*,
    types::{QueuePosition, State},
};

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary> {
    let (state, storage) = crate::types::State::load(deps, env)?;
    match msg {
        QueryMsg::Config {} => to_json_binary(&state.config),
        QueryMsg::Balance { address } => todo!(),
        QueryMsg::Status { start_after, limit } => todo!(),
        QueryMsg::HasWork {} => todo!(),
        QueryMsg::QueueStatus {
            address,
            start_after,
            limit,
        } => {
            let wallet = address.validate(state.api)?;
            let response = queue_status(state, storage, wallet, start_after, limit)?;
            to_json_binary(&response)
        }
    }
    .map_err(anyhow::Error::from)
}

const DEFAULT_QUERY_LIMIT: u32 = 10;

fn queue_status(
    state: State,
    storage: &dyn Storage,
    wallet: Addr,
    start_after: Option<QueuePositionId>,
    limit: Option<u32>,
) -> Result<QueueResp> {
    let items = crate::state::WALLET_QUEUE_ITEMS.prefix(&wallet).range(
        storage,
        None,
        start_after.map(Bound::exclusive),
        Order::Descending,
    );
    let limit = usize::try_from(
        limit
            .unwrap_or(DEFAULT_QUERY_LIMIT)
            .min(DEFAULT_QUERY_LIMIT),
    )?;
    let mut response = vec![];
    let processed_till = crate::state::LAST_PROCESSED_QUEUE_ID.may_load(storage)?;
    for item in items.take(limit) {
        let (queue_position, _) = item?;
        let item = crate::state::PENDING_QUEUE_ITEMS
            .may_load(storage, &queue_position)?
            .expect("Logic error in queue_status: PENDING_QUEUE_ITEMS.may_load returned None");
        let item = item.into_queue_resp_item(queue_position);
        response.push(item)
    }
    Ok(QueueResp {
        items: response,
        processed_till,
    })
}
