use cosmwasm_std::Reply;

use crate::prelude::*;

pub(crate) const REPLY_ID_OPEN_POSITION: u64 = 0;

#[entry_point]
pub fn reply(_deps: DepsMut, _env: Env, _msg: Reply) -> Result<Response> {
    println!("inside reply");
    Ok(Response::new().add_event(Event::new("reply")))
}
