use cosmwasm_std::from_binary;
use msg::token::Token;

use crate::prelude::*;

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    let (msg, received, sender) = match msg {
        ExecuteMsg::Receive {
            sender,
            amount,
            msg,
        } => {
            let sender = sender.validate(state.api)?;
            let get_lp_token = || LpToken::from_u128(amount.u128());
            let msg = from_binary(&msg)?;
            let received = Some(if info.sender == state.market_info.lp_addr {
                Received::Lp(get_lp_token()?)
            } else if info.sender == state.market_info.xlp_addr {
                Received::Xlp(get_lp_token()?)
            } else {
                match &state.market_info.collateral {
                    Token::Cw20 {
                        addr,
                        decimal_places: _,
                    } => {
                        if addr.as_str() == info.sender.as_str() {
                            Received::Collateral(Collateral::from_decimal256(
                                state.market_info.collateral.from_u128(amount.into())?,
                            ))
                        } else {
                            anyhow::bail!("Invalid Receive called from contract {}", info.sender)
                        }
                    }
                    Token::Native { .. } => anyhow::bail!(
                        "Invalid Receive for native collateral market from contract {}",
                        info.sender
                    ),
                }
            });

            (msg, received, sender)
        }
        msg => {
            let received = None;
            (msg, received, info.sender)
        }
    };

    match msg {
        ExecuteMsg::Owner(_) => todo!(),
        ExecuteMsg::Receive { .. } => anyhow::bail!("Cannot have double-wrapped Receive"),
        ExecuteMsg::LockdropDeposit { .. } => todo!(),
        ExecuteMsg::LockdropWithdraw { .. } => todo!(),
        ExecuteMsg::Deposit {} => {
            let received = received.context("Must send collateral, LP, or xLP for a deposit")?;
            state.deposit(&mut ctx, &sender, received)?;
        }
        ExecuteMsg::Withdraw { amount } => state.withdraw(&mut ctx, &sender, amount)?,
        ExecuteMsg::ClaimLvn {} => todo!(),
        ExecuteMsg::Reinvest {} => todo!(),
        ExecuteMsg::TransferBonus {} => todo!(),
    }

    Ok(ctx.response.into_response())
}

#[derive(Debug, Clone, Copy)]
enum Received {
    Collateral(Collateral),
    Lp(LpToken),
    Xlp(LpToken),
}

impl State<'_> {
    fn deposit(&self, ctx: &mut StateContext, farmer: &Addr, received: Received) -> Result<()> {
        let xlp = match received {
            Received::Collateral(_) => todo!(),
            Received::Lp(_) => todo!(),
            Received::Xlp(xlp) => xlp,
        };

        self.farming_deposit(ctx, farmer, xlp)?;

        Ok(())
    }

    fn withdraw(
        &self,
        ctx: &mut StateContext,
        farmer: &Addr,
        amount: Option<NonZero<FarmingToken>>,
    ) -> Result<()> {
        let token = Token::Cw20 {
            addr: self.market_info.xlp_addr.clone().into(),
            decimal_places: LpToken::PRECISION,
        };

        let xlp = self.farming_withdraw(ctx, farmer, amount)?;
        let msg = msg::contracts::liquidity_token::entry::ExecuteMsg::Transfer {
            recipient: farmer.into(),
            amount: token
                .into_u128(xlp.into_decimal256())?
                .context("Invalid transfer amount calculated")?
                .into(),
        };
        ctx.response
            .add_execute_submessage_oneshot(&self.market_info.xlp_addr, &msg)?;
        Ok(())
    }
}
