use anyhow::bail;
use cosmwasm_std::Reply;
use msg::contracts::market::deferred_execution::DeferredExecId;

use crate::{prelude::*, types::State};

pub(crate) const REPLY_ID_OPEN_POSITION: u64 = 0;

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    println!("inside reply");
    let (_state, storage) = State::load_mut(deps, &env)?;
    if msg.id == REPLY_ID_OPEN_POSITION {
        println!("foo: {:?}", msg.result);
        let deferred_exec_id: DeferredExecId = match msg.result {
            cosmwasm_std::SubMsgResult::Ok(res) => res
                .events
                .iter()
                .find(|e| e.ty == "wasm-deferred-exec-queued")
                .context("No wasm-deferred-exec-queued event found")?
                .attributes
                .iter()
                .find(|a| a.key == "deferred-exec-id")
                .context("No deferred-exec-id found in wasm-deferred-exec-queued event")?
                .value
                .parse()?,
            cosmwasm_std::SubMsgResult::Err(e) => {
                println!("reply failed!");
                bail!("Submessage reply received an error: {e}")
            } ,
        };
        crate::state::REPLY_DEFERRED_EXEC_ID.save(storage, &Some(deferred_exec_id))?;
    }
    Ok(Response::new().add_event(Event::new("reply")))
}
