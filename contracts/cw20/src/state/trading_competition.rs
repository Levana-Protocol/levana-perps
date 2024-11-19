use crate::state::*;
use perpswap::contracts::cw20::entry::ExecuteMsg;
use perpswap::prelude::*;

/// The market address, if we're limiting transactions for the trading competition
///
/// If not present, we have an unrestricted token
const MARKET_ADDRESS: Item<Addr> = Item::new(namespace::TC_MARKET_ADDRESS);

impl State<'_> {
    // the protocol takes custody of funds as part of normal operation
    // so we must allow transfers to and from protocol contracts
    // everyone else is denied
    pub(crate) fn assert_trading_competition(
        &self,
        ctx: &mut StateContext,
        sender: &Addr,
        msg: &ExecuteMsg,
    ) -> Result<()> {
        let market_address = match MARKET_ADDRESS.may_load(ctx.storage)? {
            Some(market_address) => market_address,
            None => return Ok(()),
        };

        if sender == market_address {
            return Ok(());
        }

        let recipient = match msg {
            ExecuteMsg::Transfer { recipient, .. } => recipient,
            ExecuteMsg::Send { contract, .. } => contract,
            ExecuteMsg::IncreaseAllowance { spender, .. } => spender,
            ExecuteMsg::DecreaseAllowance { spender, .. } => spender,
            ExecuteMsg::TransferFrom { recipient, .. } => recipient,
            ExecuteMsg::SendFrom { contract, .. } => contract,
            _ => return Ok(()),
        }
        .validate(self.api)?;

        if market_address == recipient {
            Ok(())
        } else {
            Err(
                anyhow!(PerpError::cw20(ErrorId::Auth,
                    format!("failed auth, market: {}, sender_addr: {}, recipient: {:?}, reason: trading competition",
                        market_address,
                        sender,
                        recipient))))
        }
    }

    pub(crate) fn set_market_addr(&self, ctx: &mut StateContext, market: &Addr) -> Result<()> {
        anyhow::ensure!(MARKET_ADDRESS.may_load(ctx.storage)? == None);
        MARKET_ADDRESS.save(ctx.storage, market)?;
        Ok(())
    }
}
