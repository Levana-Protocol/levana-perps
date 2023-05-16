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
                            Received::Collateral(
                                NonZero::<Collateral>::try_from_decimal(
                                    state.market_info.collateral.from_u128(amount.into())?,
                                )
                                .context("collateral must be non-zero")?,
                            )
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
            let token = state.market_info.collateral.clone();
            let received = match &token {
                Token::Native { denom, .. } => info
                    .funds
                    .iter()
                    .find_map(|coin| {
                        if coin.denom == *denom {
                            let amount = token
                                .from_u128(coin.amount.u128())
                                .ok()
                                .and_then(NonZero::<Collateral>::try_from_decimal)
                                .map(Received::Collateral);

                            match amount {
                                Some(amount) => Some(Ok(amount)),
                                None => Some(Err(anyhow::anyhow!(
                                    "Invalid collateral amount {}",
                                    coin.amount
                                ))),
                            }
                        } else {
                            None
                        }
                    })
                    .transpose()?,
                Token::Cw20 { .. } => None,
            };

            (msg, received, info.sender)
        }
    };

    state.validate_period_msg(ctx.storage, &sender, &msg)?;

    match msg {
        ExecuteMsg::Owner(owner_msg) => {
            state.validate_admin(ctx.storage, &sender)?;
            match owner_msg {
                OwnerExecuteMsg::StartLockdropPeriod { start } => {
                    state.start_lockdrop_period(&mut ctx, start)?
                }
                OwnerExecuteMsg::StartLaunchPeriod { start } => {
                    state.start_launch_period(&mut ctx, start)?
                }
                OwnerExecuteMsg::SetEmissions { .. } => todo!(),
                OwnerExecuteMsg::ClearEmissions {} => todo!(),
                OwnerExecuteMsg::UpdateConfig { .. } => todo!(),
            }
        }
        ExecuteMsg::Receive { .. } => anyhow::bail!("Cannot have double-wrapped Receive"),
        ExecuteMsg::LockdropDeposit { bucket_id } => {
            if let Some(Received::Collateral(amount)) = received {
                state.lockdrop_deposit(&mut ctx, sender, bucket_id, amount)?;
            } else {
                anyhow::bail!("Must send collateral for a lockdrop deposit");
            }
        }
        ExecuteMsg::LockdropWithdraw { bucket_id, amount } => {
            state.lockdrop_withdraw(&mut ctx, sender, bucket_id, amount)?;
        }
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
    Collateral(NonZero<Collateral>),
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

        let farming = self.farming_deposit(ctx, farmer, xlp)?;

        ctx.response.add_event(DepositEvent {
            farmer: farmer.clone(),
            farming,
            xlp,
            source: match received {
                Received::Collateral(_) => DepositSource::Collateral,
                Received::Lp(_) => DepositSource::Lp,
                Received::Xlp(_) => DepositSource::Xlp,
            },
        });

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

        let (xlp, farming) = self.farming_withdraw(ctx, farmer, amount)?;
        let msg = msg::contracts::liquidity_token::entry::ExecuteMsg::Transfer {
            recipient: farmer.into(),
            amount: token
                .into_u128(xlp.into_decimal256())?
                .context("Invalid transfer amount calculated")?
                .into(),
        };
        ctx.response
            .add_execute_submessage_oneshot(&self.market_info.xlp_addr, &msg)?;
        ctx.response.add_event(WithdrawEvent {
            farmer: farmer.clone(),
            farming,
            xlp,
        });
        Ok(())
    }
}
