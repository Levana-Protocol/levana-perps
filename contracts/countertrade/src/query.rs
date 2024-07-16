use crate::prelude::*;

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary> {
    let config = crate::state::CONFIG
        .load(deps.storage)
        .context("Could not load config")?;
    match msg {
        QueryMsg::Config {} => to_json_binary(&config).context("Unable to render Config to JSON"),
        QueryMsg::Balance {
            address,
            start_after,
        } => todo!(),
        QueryMsg::HasWork { market } => todo!(),
    }
}
