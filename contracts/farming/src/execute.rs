use cosmwasm_std::{wasm_execute, Reply, SubMsg};
use msg::prelude::MarketExecuteMsg::DepositLiquidity;
use msg::token::Token;

use crate::state::reply::{DepositReplyData, ReplyId, EPHEMERAL_DEPOSIT_COLLATERAL_DATA};
use crate::{prelude::*, state::funds::Received};

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;
    let (sender, received, msg) = state.funds_received(info.clone(), msg)?;

    state.validate_period_msg(ctx.storage, &sender, &msg)?;

    match msg {
        ExecuteMsg::Owner(owner_msg) => {
            state.validate_admin(ctx.storage, &sender)?;
            match owner_msg {
                OwnerExecuteMsg::StartLockdropPeriod { start } => {
                    state.start_lockdrop_period(&mut ctx, start)?
                }
                OwnerExecuteMsg::StartLaunchPeriod {} => state.start_launch_period(&mut ctx)?,
                OwnerExecuteMsg::SetEmissions {
                    start,
                    duration,
                    lvn,
                } => {
                    let received_lvn = state.get_lvn_funds(&info, ctx.storage)?;

                    anyhow::ensure!(
                        received_lvn == lvn.raw(),
                        "LVN amount {} does not match sent LVN funds {}",
                        lvn,
                        received_lvn
                    );

                    state.set_emissions(
                        &mut ctx,
                        start.unwrap_or_else(|| state.now()),
                        Duration::from_seconds(duration.into()),
                        lvn,
                    )?
                }
                OwnerExecuteMsg::ClearEmissions {} => state.clear_emissions(&mut ctx)?,
                OwnerExecuteMsg::ReclaimEmissions { .. } => todo!(),
                OwnerExecuteMsg::SetLockdropRewards { lvn } => {
                    let received_lvn = state.get_lvn_funds(&info, ctx.storage)?;

                    anyhow::ensure!(
                        received_lvn == lvn.raw(),
                        "LVN amount {} does not match sent LVN funds {}",
                        lvn,
                        received_lvn
                    );

                    state.save_lockdrop_rewards(ctx.storage, received_lvn)?
                }
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
        ExecuteMsg::ClaimLockdropRewards {} => state.claim_lockdrop_rewards(&mut ctx, &sender)?,
        ExecuteMsg::ClaimEmissions {} => state.claim_lvn_emissions(&mut ctx, &sender)?,
        ExecuteMsg::Reinvest {} => state.reinvest_yield(&mut ctx)?,
        ExecuteMsg::TransferBonus {} => state.transfer_bonus(&mut ctx)?,
    }

    Ok(ctx.response.into_response())
}

#[entry_point]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response> {
    let (state, mut ctx) = StateContext::new(deps, env)?;

    match ReplyId::try_from(msg.id) {
        Ok(id) => match id {
            ReplyId::TransferCollateral => {
                state.handle_transfer_collateral_reply(&mut ctx)?;
            }
            ReplyId::ReinvestYield => {
                state.handle_reinvest_yield_reply(ctx.storage)?;
            }
            ReplyId::FarmingDeposit => {
                state.handle_farming_deposit_reply(&mut ctx)?;
            }
        },
        _ => {
            return Err(perp_anyhow!(
                ErrorId::InternalReply,
                ErrorDomain::Farming,
                "not a valid reply id: {}",
                msg.id
            ));
        }
    };

    Ok(ctx.response.into_response())
}

impl State<'_> {
    fn deposit(&self, ctx: &mut StateContext, farmer: &Addr, received: Received) -> Result<()> {
        match received {
            Received::Collateral(collateral) => {
                let deposit_liquidity_msg = self.market_info.collateral.into_execute_msg(
                    &self.market_info.addr.clone(),
                    collateral.raw(),
                    &DepositLiquidity { stake_to_xlp: true },
                )?;

                let xlp = self.query_xlp_balance()?;
                EPHEMERAL_DEPOSIT_COLLATERAL_DATA.save(
                    ctx.storage,
                    &DepositReplyData {
                        farmer: farmer.clone(),
                        xlp_balance_before: xlp,
                    },
                )?;

                ctx.response.add_raw_submessage(SubMsg::reply_on_success(
                    deposit_liquidity_msg,
                    ReplyId::FarmingDeposit.into(),
                ));
            }
            Received::Lp(lp) => {
                let lp_amount = NonZero::try_from_decimal(lp.into_decimal256())
                    .with_context(|| "unable to convert lp amount")?;
                let stake_lp_msg = &MarketExecuteMsg::StakeLp {
                    amount: Some(lp_amount),
                };

                let xlp = self.query_xlp_balance()?;
                EPHEMERAL_DEPOSIT_COLLATERAL_DATA.save(
                    ctx.storage,
                    &DepositReplyData {
                        farmer: farmer.clone(),
                        xlp_balance_before: xlp,
                    },
                )?;

                ctx.response.add_raw_submessage(SubMsg::reply_on_success(
                    wasm_execute(self.market_info.addr.to_string(), &stake_lp_msg, vec![])?,
                    ReplyId::FarmingDeposit.into(),
                ));
            }
            Received::Xlp(xlp) => {
                let (farming, pool_size) = self.farming_deposit(ctx, farmer, xlp)?;

                ctx.response.add_event(DepositEvent {
                    farmer: farmer.clone(),
                    farming,
                    xlp,
                    source: DepositSource::Collateral,
                    pool_size,
                });
            }
        }

        Ok(())
    }

    fn handle_farming_deposit_reply(&self, ctx: &mut StateContext) -> Result<()> {
        let ephemeral_data = EPHEMERAL_DEPOSIT_COLLATERAL_DATA.load_once(ctx.storage)?;
        let new_balance = self.query_xlp_balance()?;
        let delta = new_balance.checked_sub(ephemeral_data.xlp_balance_before)?;
        self.farming_deposit(ctx, &ephemeral_data.farmer, delta)?;

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

        let (xlp, farming, pool_size) = self.farming_withdraw(ctx, farmer, amount)?;
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
            pool_size
        });

        Ok(())
    }

    fn set_emissions(
        &self,
        ctx: &mut StateContext,
        start: Timestamp,
        duration: Duration,
        lvn: NonZero<LvnToken>,
    ) -> Result<()> {
        let old_emissions = self.may_load_lvn_emissions(ctx.storage)?;
        let new_emissions = Emissions {
            start,
            end: start + duration,
            lvn,
        };

        match old_emissions {
            None => self.save_lvn_emissions(ctx.storage, Some(new_emissions))?,
            Some(old_emissions) => {
                anyhow::ensure!(
                    self.now() > old_emissions.end,
                    "Unable to save new emissions while previous emissions are ongoing"
                );

                self.update_emissions_per_token(ctx, &old_emissions)?;
                self.save_lvn_emissions(ctx.storage, Some(new_emissions))?;
            }
        }

        Ok(())
    }

    fn clear_emissions(&self, ctx: &mut StateContext) -> Result<()> {
        if let Some(emissions) = self.may_load_lvn_emissions(ctx.storage)? {
            self.update_emissions_per_token(ctx, &emissions)?;
        }

        self.save_lvn_emissions(ctx.storage, None)?;

        Ok(())
    }
}
