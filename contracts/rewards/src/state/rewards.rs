use crate::state::{State, StateContext};
use cosmwasm_std::{Addr, CosmosMsg, Decimal256};
use cw_storage_plus::Map;
use msg::contracts::rewards::entry::events::{ClaimRewardsEvent, GrantRewardsEvent};
use msg::token::Token;
use serde::{Deserialize, Serialize};
use shared::prelude::*;

const REWARDS: Map<&Addr, RewardsInfo> = Map::new("rewards");

/// A struct containing information pertaining to rewards granted to a single user
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RewardsInfo {
    /// The amount of tokens rewarded to the user
    pub amount: Decimal256,
    /// The start time of the unlocking period
    pub start: Timestamp,
    /// The duration of the unlocking period
    pub duration: Duration,
    /// The amount of tokens that have already been claimed (and transferred) to the user
    pub claimed: Decimal256,
    /// The timestamp of the last time the user claimed rewards
    pub last_claimed: Timestamp,
}

impl RewardsInfo {
    pub fn calculate_unlocked_rewards(&self, from: Timestamp) -> Result<Decimal256> {
        let unlocked = if from >= self.start + self.duration {
            self.amount.checked_sub(self.claimed)?
        } else {
            let elapsed = from.checked_sub(self.last_claimed, "calculate_unlocked_rewards")?;
            let duration = Decimal256::from_atomics(self.duration.as_nanos(), 0)?;

            Decimal256::from_atomics(elapsed.as_nanos(), 0)?
                .checked_mul(self.amount)?
                .checked_div(duration)?
                .min(self.amount.checked_sub(self.claimed)?)
        };

        Ok(unlocked)
    }
}

impl State<'_> {
    fn create_transfer_msg(
        &self,
        token_denom: String,
        address: &Addr,
        amount: Decimal256,
    ) -> Result<CosmosMsg> {
        let token = Token::Native {
            denom: token_denom,
            decimal_places: 6,
        };

        let amount = NonZero::<Collateral>::try_from_decimal(amount)
            .with_context(|| "failed to convert rewards for transfer")?;
        token
            .into_transfer_msg(address, amount)?
            .with_context(|| "failed to create rewards transfer msg")
    }

    pub fn load_rewards(
        &self,
        storage: &dyn Storage,
        address: &Addr,
    ) -> Result<Option<RewardsInfo>> {
        let rewards = REWARDS.may_load(storage, address)?;

        Ok(rewards)
    }

    pub(crate) fn grant_rewards(
        &self,
        ctx: &mut StateContext,
        addr: Addr,
        amount: NonZero<LvnToken>,
    ) -> Result<()> {
        let transfer_amount = amount
            .into_decimal256()
            .checked_mul(self.config.immediately_transferable)?;
        let locked_amount = amount.into_decimal256().checked_sub(transfer_amount)?;

        let rewards_info = self.load_rewards(ctx.storage, &addr)?;
        let (transfer_amount, locked_amount) = match rewards_info {
            None => (transfer_amount, locked_amount),
            Some(rewards_info) => {
                /*  Handling the case where the specified address already has vesting rewards by
                   1. Combining whatever has been unlocked with the immediately transferable part of
                      the new rewards
                   2. Combining the (locked) remainder of the existing rewards with the rest of the
                      new rewards
                */

                let unlocked = rewards_info.calculate_unlocked_rewards(self.now())?;
                let new_transfer_amount = transfer_amount.checked_add(unlocked)?;
                let new_locked_amount = rewards_info
                    .amount
                    .checked_sub(unlocked)?
                    .checked_sub(rewards_info.claimed)?
                    .checked_add(locked_amount)?;

                (new_transfer_amount, new_locked_amount)
            }
        };

        ctx.response_mut().add_message(self.create_transfer_msg(
            self.config.token_denom.clone(),
            &addr,
            transfer_amount,
        )?);

        // Store the remainder of the rewards

        let rewards_info = RewardsInfo {
            amount: locked_amount,
            start: self.now(),
            duration: Duration::from_seconds(self.config.unlock_duration_seconds.into()),
            claimed: Decimal256::zero(),
            last_claimed: self.now(),
        };

        REWARDS.save(ctx.storage, &addr, &rewards_info)?;

        ctx.response.add_event(GrantRewardsEvent {
            address: addr,
            amount: amount.into_decimal256(),
        });

        Ok(())
    }

    pub(crate) fn claim_rewards(&self, ctx: &mut StateContext, address: Addr) -> Result<()> {
        let rewards_info = self.load_rewards(ctx.storage, &address)?;

        match rewards_info {
            None => bail!("There are no outstanding rewards for {}", address),
            Some(mut rewards_info) => {
                let unlocked = rewards_info.calculate_unlocked_rewards(self.now())?;

                if unlocked.is_zero() {
                    bail!("There are no outstanding rewards for {}", address);
                }

                ctx.response_mut().add_message(self.create_transfer_msg(
                    self.config.token_denom.clone(),
                    &address,
                    unlocked,
                )?);

                rewards_info.claimed = rewards_info.claimed.checked_add(unlocked)?;
                rewards_info.last_claimed = self.now();

                if rewards_info.claimed == rewards_info.amount {
                    REWARDS.remove(ctx.storage, &address);
                } else {
                    REWARDS.save(ctx.storage, &address, &rewards_info)?;
                }

                ctx.response.add_event(ClaimRewardsEvent {
                    address,
                    amount: unlocked,
                });
            }
        };

        Ok(())
    }
}
