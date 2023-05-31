use crate::prelude::*;
use crate::state::farming::RawFarmerStats;
use cosmwasm_std::{BankMsg, CosmosMsg};
use cw_storage_plus::Item;
use msg::token::Token;
use std::cmp::{max, min};

/// The LVN token used for rewards
const LVN_TOKEN: Item<Token> = Item::new(namespace::LVN_TOKEN);

//todo don't forget to set LVN_LOCKDROP_REWARDS
/// The total amount of LVN rewards designated for lockdrop participants
const LVN_LOCKDROP_REWARDS: Item<LvnToken> = Item::new(namespace::LVN_LOCKDROP_REWARDS);

/// Tracks how much reward is allocated per token at a given timestamp
///
/// REWARDS_PER_TIME_PER_TOKEN is structured as a prefix sum data series where each value in the map
/// represents part of a formula that is used to calculate how many reward tokens a farmer gets at
/// any given time. The value is calculated as follows:
///
/// total_rewards / total_farming_tokens * elapsed_ratio
///
/// where
/// * total_rewards - the total amount of LVN included in the current emissions period
/// * total_farming_tokens - the total amount of farming tokens currently minted (see [RawFarmerStats])
/// * elapsed_ratio - the amount of time that has elapsed since the last entry relative to the duration
///                   of the current emissions period
///
/// When the amount of farming tokens changes (i.e. on a deposit or withdrawal), this value is added
/// to the previous entry and inserted into the Map, thereby allowing us to calculate the value of
/// a farming token for any given interval.
const EMISSIONS_PER_TIME_PER_TOKEN: Map<Timestamp, LvnToken> =
    Map::new(namespace::REWARDS_PER_TIME_PER_TOKEN);

/// The active LVN emission plan
const LVN_EMISSIONS: Item<Emissions> = Item::new(namespace::LVN_EMISSIONS);

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct LockdropConfig {
    pub(crate) lockdrop_lvn_unlock_seconds: Duration,
    pub(crate) lockdrop_immediate_unlock_ratio: Decimal256,
}

const LOCKDROP_CONFIG: Item<LockdropConfig> = Item::new(namespace::LOCKDROP_CONFIG);

impl State<'_> {
    pub(crate) fn rewards_init(&self, store: &mut dyn Storage) -> Result<()> {
        EMISSIONS_PER_TIME_PER_TOKEN
            .save(store, self.now(), &LvnToken::zero())
            .map_err(|e| e.into())
    }

    pub(crate) fn save_lvn_token(&self, ctx: &mut StateContext, denom: String) -> Result<()> {
        let token = Token::Native {
            denom,
            decimal_places: 6,
        };

        LVN_TOKEN.save(ctx.storage, &token)?;
        Ok(())
    }

    pub(crate) fn load_lvn_token(&self, store: &dyn Storage) -> Result<Token> {
        let token = LVN_TOKEN.load(store)?;
        Ok(token)
    }

    pub(crate) fn save_lvn_emissions(
        &self,
        store: &mut dyn Storage,
        emissions: Option<Emissions>,
    ) -> Result<()> {
        match emissions {
            None => LVN_EMISSIONS.remove(store),
            Some(emissions) => LVN_EMISSIONS.save(store, &emissions)?,
        }

        Ok(())
    }

    pub(crate) fn may_load_lvn_emissions(&self, store: &dyn Storage) -> Result<Option<Emissions>> {
        let emissions = LVN_EMISSIONS.may_load(store)?;
        Ok(emissions)
    }

    pub(crate) fn save_lockdrop_config(
        &self,
        store: &mut dyn Storage,
        config: LockdropConfig,
    ) -> Result<()> {
        LOCKDROP_CONFIG.save(store, &config)?;
        Ok(())
    }

    pub(crate) fn load_lockdrop_config(&self, store: &dyn Storage) -> Result<LockdropConfig> {
        let lockdrop_config = LOCKDROP_CONFIG.load(store)?;
        Ok(lockdrop_config)
    }

    /// Calculates how many reward tokens can be claimed from the lockdrop and transfers them to the
    /// specified user
    pub(crate) fn claim_lockdrop_rewards(
        &self,
        ctx: &mut StateContext,
        farmer: &Addr,
    ) -> Result<()> {
        let period = self.get_period_resp(ctx.storage)?;
        let lockdrop_start = match period {
            FarmingPeriodResp::Launched { started_at } => started_at,
            _ => bail!("Cannot collect lockdrop rewards prior to launch"),
        };

        // First get the total amount of LVN tokens rewarded to this lockdrop participant

        let total_lockdrop_rewards = LVN_LOCKDROP_REWARDS.load(ctx.storage)?;
        let mut stats = self.load_raw_farmer_stats(ctx.storage, farmer)?;
        let total_user_rewards = total_lockdrop_rewards
            .checked_mul_dec(stats.lockdrop_farming_tokens.into_decimal256())?;

        // Next, calculate how many tokens have unlocked

        let lockdrop_config = self.load_lockdrop_config(ctx.storage)?;
        let elapsed_since_start = self
            .now()
            .checked_sub(lockdrop_start, "claim_lockdrop_rewards")?;

        let amount = if elapsed_since_start >= lockdrop_config.lockdrop_lvn_unlock_seconds {
            total_user_rewards.checked_sub(stats.lockdrop_amount_collected)?
        } else {
            let elapsed_since_last_collected = (self.now().checked_sub(
                stats.lockdrop_last_collected,
                "claim_lockdrop_rewards, elapsed_since_last_collected",
            )?)
            .as_nanos();

            let elapsed_ratio = Decimal256::from_ratio(
                elapsed_since_last_collected,
                lockdrop_config.lockdrop_lvn_unlock_seconds.as_nanos(),
            );

            total_user_rewards
                .checked_mul_dec(elapsed_ratio)?
                // using min as an added precaution to make sure it never goes above the total due to rounding errors
                .min(total_user_rewards.checked_sub(stats.lockdrop_amount_collected)?)
        };

        // Update internal storage & transfer

        stats.lockdrop_amount_collected.checked_add(amount)?;
        stats.lockdrop_last_collected = self.now();
        self.save_raw_farmer_stats(ctx, farmer, &stats)?;

        let amount = NumberGtZero::new(amount.into_decimal256())
            .context("Unable to convert amount into NumberGtZero")?;
        let coin = self
            .load_lvn_token(ctx.storage)?
            .into_native_coin(amount)?
            .context("Invalid LVN transfer amount calculated")?;

        let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
            to_address: farmer.to_string(),
            amount: vec![coin],
        });

        ctx.response.add_message(transfer_msg);

        Ok(())
    }

    /// Get the latest key and value from [REWARDS_PER_TIME_PER_TOKEN].
    fn latest_emissions_per_token(&self, store: &dyn Storage) -> Result<(Timestamp, LvnToken)> {
        EMISSIONS_PER_TIME_PER_TOKEN
            .range(store, None, None, Order::Descending)
            .next()
            .expect("REWARDS_PER_TIME_PER_TOKEN cannot be empty")
            .map_err(|e| e.into())
    }

    /// Calculates emissions per farming tokens ratio since the last update
    pub fn calculate_emissions_per_token_per_time(
        &self,
        store: &dyn Storage,
        emissions: &Emissions,
    ) -> Result<LvnToken> {
        let emissions_duration = Decimal256::from_ratio(
            emissions
                .end
                .checked_sub(emissions.start, "emissions_duration")?
                .as_nanos(),
            1u64,
        );
        let (latest_timestamp, latest_rewards_per_token) =
            self.latest_emissions_per_token(store)?;
        let total_farming_tokens = self.load_farming_totals(store)?.farming;

        let rewards_per_token = if total_farming_tokens.is_zero() {
            LvnToken::zero()
        } else {
            let total_lvn = emissions.lvn.raw();
            let start_time = max(latest_timestamp, emissions.start);
            let end_time = min(self.now(), emissions.end);
            let elapsed_time =
                end_time.checked_sub(start_time, "calculate_rewards_per_farming_token")?;
            let elapsed_time = Decimal256::from_ratio(elapsed_time.as_nanos(), 1u64);
            let elapsed_ratio = elapsed_time.checked_div(emissions_duration)?;

            total_lvn
                .checked_div_dec(total_farming_tokens.into_decimal256())?
                .checked_mul_dec(elapsed_ratio)?
        };

        let new_rewards_per_token = latest_rewards_per_token.checked_add(rewards_per_token)?;

        Ok(new_rewards_per_token)
    }

    /// Performs bookkeeping on any pending values for a farmer
    pub(crate) fn farming_perform_emissions_bookkeeping(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
    ) -> Result<()> {
        if let Some(emissions) = self.may_load_lvn_emissions(ctx.storage)? {
            self.update_emissions_per_token(ctx, &emissions)?;
            self.update_accrued_emissions(ctx, addr, &emissions)?;
        }

        Ok(())
    }

    /// Calculates the amount of unlocked emissions that are available from the last time emissions
    /// were claimed or accrued
    pub(crate) fn calculate_unlocked_emissions(
        &self,
        store: &dyn Storage,
        farmer_stats: &RawFarmerStats,
        emissions: &Emissions,
    ) -> Result<LvnToken> {
        let start_prefix_sum = farmer_stats.xlp_last_claimed_prefix_sum;
        let end_prefix_sum = self.calculate_emissions_per_token_per_time(store, emissions)?;
        let unlocked_emissions = end_prefix_sum
            .checked_sub(start_prefix_sum)?
            .checked_mul_dec(farmer_stats.total_farming_tokens()?.into_decimal256())?;

        Ok(unlocked_emissions)
    }

    /// Updates the emissions per farming token ratio
    pub(crate) fn update_emissions_per_token(
        &self,
        ctx: &mut StateContext,
        emissions: &Emissions,
    ) -> Result<()> {
        let rewards_per_token =
            self.calculate_emissions_per_token_per_time(ctx.storage, emissions)?;

        EMISSIONS_PER_TIME_PER_TOKEN.save(ctx.storage, self.now(), &rewards_per_token)?;

        Ok(())
    }

    /// Allocates accrued emissions to the specified user
    pub(crate) fn update_accrued_emissions(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
        emissions: &Emissions,
    ) -> Result<()> {
        let mut farmer_stats = self.load_raw_farmer_stats(ctx.storage, addr)?;
        let accrued_rewards =
            self.calculate_unlocked_emissions(ctx.storage, &farmer_stats, emissions)?;

        farmer_stats.accrued_emissions = farmer_stats
            .accrued_emissions
            .checked_add(accrued_rewards)?;
        let end_prefix_sum = self.calculate_emissions_per_token_per_time(ctx.storage, emissions)?;
        farmer_stats.xlp_last_claimed_prefix_sum = end_prefix_sum;

        self.save_raw_farmer_stats(ctx, addr, &farmer_stats)?;

        Ok(())
    }

    /// Calculates how many tokens the user can claim from LVN emissions
    /// and transfers them to the specified user
    pub(crate) fn claim_lvn_emissions(&self, ctx: &mut StateContext, addr: &Addr) -> Result<()> {
        let mut farmer_stats = self.load_raw_farmer_stats(ctx.storage, addr)?;

        let emissions = self.may_load_lvn_emissions(ctx.storage)?;
        let unlocked_rewards = match emissions {
            None => LvnToken::zero(),
            Some(emissions) => {
                if farmer_stats.xlp_farming_tokens.is_zero() {
                    LvnToken::zero()
                } else {
                    let unlocked =
                        self.calculate_unlocked_emissions(ctx.storage, &farmer_stats, &emissions)?;
                    let end_prefix_sum =
                        self.calculate_emissions_per_token_per_time(ctx.storage, &emissions)?;

                    farmer_stats.xlp_last_claimed_prefix_sum = end_prefix_sum;

                    unlocked
                }
            }
        };

        let amount = unlocked_rewards.checked_add(farmer_stats.accrued_emissions)?;

        farmer_stats.accrued_emissions = LvnToken::zero();
        self.save_raw_farmer_stats(ctx, addr, &farmer_stats)?;

        let amount = NumberGtZero::new(amount.into_decimal256())
            .context("Unable to convert amount into NumberGtZero")?;
        let coin = self
            .load_lvn_token(ctx.storage)?
            .into_native_coin(amount)?
            .context("Invalid LVN transfer amount calculated")?;

        let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
            to_address: addr.to_string(),
            amount: vec![coin],
        });

        ctx.response.add_message(transfer_msg);

        Ok(())
    }
}
