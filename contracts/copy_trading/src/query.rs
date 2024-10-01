use crate::{prelude::*, work::get_work};

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary> {
    let (state, storage) = crate::types::State::load(deps, env)?;
    match msg {
        QueryMsg::Config {} => to_json_binary(&state.config),
        QueryMsg::Balance {
            address,
            start_after,
            limit,
        } => {
            let wallet = address.validate(state.api)?;
            let balance = balance(storage, wallet, start_after, limit)?;
            to_json_binary(&balance)
        }
        QueryMsg::Status {
            start_after: _,
            limit: _,
        } => todo!(),
        QueryMsg::HasWork {} => {
            let work = get_work(&state, storage)?;
            to_json_binary(&work)
        }
        QueryMsg::QueueStatus {
            address,
            start_after,
            limit,
        } => {
            let wallet = address.validate(state.api)?;
            let response = queue_status(storage, wallet, start_after, limit)?;
            to_json_binary(&response)
        }
    }
    .map_err(anyhow::Error::from)
}

const DEFAULT_QUERY_LIMIT: u32 = 10;

fn balance(
    storage: &dyn Storage,
    wallet: Addr,
    start_after: Option<Token>,
    limit: Option<u32>,
) -> Result<BalanceResp> {
    let limit = usize::try_from(
        limit
            .unwrap_or(DEFAULT_QUERY_LIMIT)
            .min(DEFAULT_QUERY_LIMIT),
    )?;
    let wallets = crate::state::SHARES
        .prefix(wallet)
        .range(
            storage,
            None,
            start_after.map(Bound::exclusive),
            Order::Descending,
        )
        .take(limit);
    let response = wallets
        .map(|item| item.map(|(token, shares)| BalanceRespItem { shares, token }))
        .collect::<cosmwasm_std::StdResult<Vec<_>>>()?;
    let start_after = response
        .last()
        .map(|item: &BalanceRespItem| item.token.clone());
    Ok(BalanceResp {
        balance: response,
        start_after,
    })
}

fn queue_status(
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
    let inc_processed_till = crate::state::LAST_PROCESSED_INC_QUEUE_ID.may_load(storage)?;
    let dec_processed_till = crate::state::LAST_PROCESSED_DEC_QUEUE_ID.may_load(storage)?;
    for item in items.take(limit) {
        let (queue_position, _) = item?;
        match queue_position {
            QueuePositionId::IncQueuePositionId(id) => {
                let item = crate::state::COLLATERAL_INCREASE_QUEUE
                    .may_load(storage, &id)?
                    .expect(
                        "Logic error in queue_status: PENDING_QUEUE_ITEMS.may_load returned None",
                    );
                let item = item.into_queue_item(id);
                response.push(item)
            }
            QueuePositionId::DecQueuePositionId(id) => {
                let item = crate::state::COLLATERAL_DECREASE_QUEUE
                    .may_load(storage, &id)?
                    .expect(
                        "Logic error in queue_status: PENDING_QUEUE_ITEMS.may_load returned None",
                    );
                let item = item.into_queue_item(id);
                response.push(item)
            }
        }
    }
    Ok(QueueResp {
        items: response,
        inc_processed_till,
        dec_processed_till,
    })
}
