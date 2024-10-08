use anyhow::Result;
use cosmwasm_std::{Addr, Uint128};
use cw_multi_test::{AppResponse, Executor};
use perpswap::contracts::cw20::entry::ExecuteMsg as Cw20ExecuteMsg;

use super::PerpsApp;

impl PerpsApp {
    pub(crate) fn cw20_exec(
        &mut self,
        sender: &Addr,
        addr: &Addr,
        msg: &Cw20ExecuteMsg,
    ) -> Result<AppResponse> {
        self.execute_contract(sender.clone(), addr.clone(), msg, &[])
    }

    pub(crate) fn cw20_mint(
        &mut self,
        contract: &Addr,
        minter: &Addr,
        user: &Addr,
        amount: Uint128,
    ) -> Result<AppResponse> {
        self.cw20_exec(
            minter,
            contract,
            &Cw20ExecuteMsg::Mint {
                recipient: user.into(),
                amount,
            },
        )
    }
}
