use crate::prelude::*;

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, _info: MessageInfo, _msg: ExecuteMsg) -> Result<Response> {
    let (_state, ctx) = StateContext::new(deps, env)?;

    // match msg {}

    Ok(ctx.response.into_response())
}
