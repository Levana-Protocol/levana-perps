use super::{State, StateContext};
use cosmwasm_std::{Binary, WasmMsg};
use shared::prelude::*;

impl State<'_> {
    pub fn send(&self, ctx: &mut StateContext, msgs: Vec<Binary>) -> Result<()> {
        for msg in msgs {
            ctx.response_mut().add_message(WasmMsg::Execute {
                contract_addr: self.config.contract.to_string(),
                msg,
                funds: vec![],
            });
        }

        Ok(())
    }
}
