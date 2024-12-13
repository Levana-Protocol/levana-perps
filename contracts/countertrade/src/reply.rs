use perpswap::contracts::market::deferred_execution::DeferredExecId;

use crate::prelude::*;

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    ensure!(msg.id == 0, "Only reply ID 0 is supported");

    let (_state, storage) = State::load_mut(deps, env)?;

    let market = crate::state::REPLY_MARKET
        .may_load(storage)?
        .context("In reply function with no REPLY_MARKET set")?;
    crate::state::REPLY_MARKET.remove(storage);

    let deferred_exec_id = match msg.result {
        cosmwasm_std::SubMsgResult::Ok(res) => {
            let deferred_exec_id: DeferredExecId = res
                .events
                .iter()
                .find(|e| e.ty == "wasm-deferred-exec-queued")
                .context("No wasm-deferred-exec-queued event found")?
                .attributes
                .iter()
                .find(|a| a.key == "deferred-exec-id")
                .context("No deferred-exec-id found in wasm-deferred-exec-queued event")?
                .value
                .parse()?;
            deferred_exec_id
        }
        cosmwasm_std::SubMsgResult::Err(e) => bail!("Submessage reply received an error: {e}"),
    };

    let mut totals = crate::state::TOTALS
        .may_load(storage, &market)?
        .context("Totals missing in reply")?;

    if let Some(old_id) = totals.deferred_exec {
        bail!(
            "Cannot handle reply with deferred exec ID {deferred_exec_id}, still waiting for {old_id}"
        );
    }

    ensure!(
        totals.deferred_collateral.is_some(),
        "Associated deferred collateral is expected for deferred exec id {deferred_exec_id}"
    );
    totals.deferred_exec = Some(deferred_exec_id);
    crate::state::TOTALS.save(storage, &market, &totals)?;
    Ok(Response::new().add_event(
        Event::new("reply")
            .add_attribute("market", market.as_str())
            .add_attribute("deferred-exec-id", deferred_exec_id.to_string()),
    ))
}
