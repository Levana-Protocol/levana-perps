use crate::prelude::*;
use cosmwasm_std::entry_point;
use cosmwasm_std::{Deps, Env};

#[entry_point]
pub fn query(deps: Deps, env: Env, _msg: QueryMsg) -> Result<QueryResponse> {
    let (_state, _store) = State::new(deps, env);
    todo!() // FIXME
            // match msg {
            //     QueryMsg::Version {} => get_contract_version(store)?.query_result(),
            // }
}
