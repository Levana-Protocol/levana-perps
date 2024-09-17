use crate::prelude::*;

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
        } => todo!(),
    }
    .map_err(anyhow::Error::from)
}
