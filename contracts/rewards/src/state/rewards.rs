use crate::state::{State, StateContext};
use cosmwasm_std::{Addr, CosmosMsg, Decimal256};
use cw_storage_plus::Map;
use msg::contracts::rewards::entry::events::{ClaimRewardsEvent, GrantRewardsEvent};
use msg::token::Token;
use serde::{Deserialize, Serialize};
use shared::prelude::*;

const REWARDS: Map<&Addr, RewardsInfo> = Map::new("rewards-info");

/// A struct containing information pertaining to rewards
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct RewardsInfo {
    /// The total amount of tokens rewarded to the user across all their hatchings
    pub(crate) total_rewards: Decimal256,
    /// The total amount of tokens claimed by the user across all their hatchings
    pub(crate) total_claimed: Decimal256,
    /// Information related to the tokens currently vesting
    pub(crate) vesting_rewards: Option<VestingRewards>,
}

/// A struct containing information pertaining to rewards from the current vesting period
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct VestingRewards {
    /// The total amount of tokens rewarded to the user for the current vesting period
    pub(crate) amount: Decimal256,
    /// The start time of the current vesting period
    pub(crate) start: Timestamp,
    /// The duration of the current vesting period
    pub(crate) duration: Duration,
    /// The amount of tokens from the current vesting period that have already been claimed
    /// (and transferred) to the user
    pub(crate) claimed: Decimal256,
    /// The timestamp of the last time the user claimed rewards
    pub(crate) last_claimed: Timestamp,
}

impl VestingRewards {
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
        let mut transfer_amount = amount
            .into_decimal256()
            .checked_mul(self.config.immediately_transferable)?;
        let mut locked_amount = amount.into_decimal256().checked_sub(transfer_amount)?;
        let mut total_rewards = amount.into_decimal256();
        let mut total_claimed = transfer_amount;
        let rewards_info = self.load_rewards(ctx.storage, &addr)?;

        /*  Handling the case where the specified address already has vesting rewards by:
           1. Combining whatever has been unlocked with the immediately transferable part of
              the new rewards
           2. Combining the (locked) remainder of the existing rewards with the rest of the
              new rewards
        */
        if let Some(rewards_info) = rewards_info {
            total_rewards = total_rewards.checked_add(rewards_info.total_rewards)?;
            total_claimed = total_claimed.checked_add(rewards_info.total_claimed)?;

            if let Some(vesting_rewards) = rewards_info.vesting_rewards {
                let unlocked = vesting_rewards.calculate_unlocked_rewards(self.now())?;

                transfer_amount = transfer_amount.checked_add(unlocked)?;
                locked_amount = vesting_rewards
                    .amount
                    .checked_sub(unlocked)?
                    .checked_sub(vesting_rewards.claimed)?
                    .checked_add(locked_amount)?;

                total_claimed = total_claimed.checked_add(unlocked)?
            }
        }

        let vesting_rewards = VestingRewards {
            amount: locked_amount,
            start: self.now(),
            duration: Duration::from_seconds(self.config.unlock_duration_seconds.into()),
            claimed: Decimal256::zero(),
            last_claimed: self.now(),
        };

        let rewards_info = RewardsInfo {
            total_rewards,
            total_claimed,
            vesting_rewards: Some(vesting_rewards),
        };

        REWARDS.save(ctx.storage, &addr, &rewards_info)?;

        ctx.response_mut().add_message(self.create_transfer_msg(
            self.config.token_denom.clone(),
            &addr,
            transfer_amount,
        )?);

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
            Some(mut rewards_info) => match rewards_info.vesting_rewards {
                None => bail!("There are no outstanding rewards for {}", address),
                Some(mut vesting_rewards) => {
                    let unlocked = vesting_rewards.calculate_unlocked_rewards(self.now())?;

                    if unlocked.is_zero() {
                        bail!("There are no outstanding rewards for {}", address);
                    }

                    rewards_info.total_claimed =
                        rewards_info.total_claimed.checked_add(unlocked)?;
                    vesting_rewards.claimed = vesting_rewards.claimed.checked_add(unlocked)?;
                    vesting_rewards.last_claimed = self.now();

                    if vesting_rewards.claimed >= vesting_rewards.amount {
                        rewards_info.vesting_rewards = None;
                    } else {
                        rewards_info.vesting_rewards = Some(vesting_rewards);
                    }

                    REWARDS.save(ctx.storage, &address, &rewards_info)?;

                    ctx.response_mut().add_message(self.create_transfer_msg(
                        self.config.token_denom.clone(),
                        &address,
                        unlocked,
                    )?);

                    ctx.response.add_event(ClaimRewardsEvent {
                        address,
                        amount: unlocked,
                    });
                }
            },
        };

        Ok(())
    }
}
