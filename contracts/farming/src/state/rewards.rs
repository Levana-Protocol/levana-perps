use crate::prelude::*;
use crate::state::farming::RawFarmerStats;
use cw_storage_plus::Item;
use msg::token::Token;

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
/// ```
/// total_rewards / total_farming_tokens * elapsed_ratio
/// ```
/// where
/// * total_rewards - the total amount of LVN included in the current emissions period
/// * total_farming_tokens - the total amount of farming tokens currently minted (see [RawFarmerStats])
/// * elapsed_ratio - the amount of time that has elapsed since the last entry relative to the duration
///                   of the current emissions period
///
/// When the amount of farming tokens changes (i.e. on a deposit or withdrawal), this value is added
/// to the previous entry and inserted into the Map, thereby allowing us to calculate the value of
/// a farming token for any given interval.
const REWARDS_PER_TIME_PER_TOKEN: Map<Timestamp, LvnToken> =
    Map::new(namespace::REWARDS_PER_TIME_PER_TOKEN);

/// The active LVN emission plan
const LVN_EMISSIONS: Item<Emissions> = Item::new(namespace::LVN_EMISSIONS);

impl State<'_> {
    pub(crate) fn rewards_init(&self, store: &mut dyn Storage) -> Result<()> {
        REWARDS_PER_TIME_PER_TOKEN
            .save(store, Timestamp::from_nanos(0), &LvnToken::zero())
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

    pub(crate) fn load_lvn_token(&self, ctx: &StateContext) -> Result<Token> {
        let token = LVN_TOKEN.load(ctx.storage)?;
        Ok(token)
    }

    pub(crate) fn save_lvn_emissions(
        &self,
        ctx: &mut StateContext,
        emissions: Emissions,
    ) -> Result<()> {
        LVN_EMISSIONS.save(ctx.storage, &emissions)?;
        Ok(())
    }

    pub(crate) fn may_load_lvn_emissions(&self, store: &dyn Storage) -> Result<Option<Emissions>> {
        let emissions = LVN_EMISSIONS.may_load(store)?;
        Ok(emissions)
    }

    /// Calculates how many reward tokens the user can collect from the lockdrop
    /// and updates internal storage accordingly
    pub(crate) fn collect_lockdrop_rewards(
        &self,
        ctx: &mut StateContext,
        farmer: &Addr,
    ) -> Result<LvnToken> {
        // First get the total amount of LVN tokens rewarded to this lockdrop participant

        let total_lockdrop_rewards = LVN_LOCKDROP_REWARDS.load(ctx.storage)?;
        let mut stats = self.load_raw_farmer_stats(ctx.storage, farmer)?; //todo should lockdrop stats be stored separately?
        let total_user_rewards = total_lockdrop_rewards
            .checked_mul_dec(stats.lockdrop_farming_tokens.into_decimal256())?;

        // Next, calculate how many tokens have unlocked

        //FIXME pull from config
        let lockdrop_start = Timestamp::from_seconds(0);
        //FIXME elapsed should be from last_claimed
        let elapsed_since_start = self
            .now()
            .checked_sub(lockdrop_start, "claim_lockdrop_rewards")?;
        //FIXME pull from config
        let unlock_duration = Duration::from_seconds(60);

        let amount = if elapsed_since_start >= unlock_duration {
            total_user_rewards.checked_sub(stats.lockdrop_amount_collected)?
        } else {
            let elapsed_since_last_collected = (self.now().checked_sub(
                stats.lockdrop_last_collected,
                "claim_lockdrop_rewards, elapsed_since_last_collected",
            )?)
            .as_nanos();

            total_user_rewards
                .checked_mul_dec(Decimal256::raw(elapsed_since_last_collected.into()))?
                .checked_div_dec(Decimal256::raw(unlock_duration.as_nanos().into()))?
                // using min as an added precaution to make sure it never goes above the total due to rounding errors
                .min(total_user_rewards)
        };

        // Lastly, update internal storage

        stats.lockdrop_amount_collected.checked_add(amount)?;
        stats.lockdrop_last_collected = self.now();
        self.save_raw_farmer_stats(ctx, farmer, &stats)?;

        Ok(amount)
    }

    /// Get the latest key and value from [REWARDS_PER_TIME_PER_TOKEN].
    fn latest_reward_per_token(&self, store: &dyn Storage) -> Result<(Timestamp, LvnToken)> {
        REWARDS_PER_TIME_PER_TOKEN
            .range(store, None, None, Order::Descending)
            .next()
            .expect("REWARDS_PER_TIME_PER_TOKEN cannot be empty")
            .map_err(|e| e.into())
    }

    /// Calculates rewards per farming tokens ratio since the last update
    fn calculate_rewards_per_token(&self, ctx: &StateContext) -> Result<LvnToken> {
        let emissions = self
            .may_load_lvn_emissions(ctx.storage)?
            .with_context(|| "There are no active emissions")?;
        let emissions_duration = Decimal256::from_ratio(
            emissions
                .end
                .checked_sub(emissions.start, "emissions_duration")?
                .as_nanos(),
            1u64,
        );
        let (latest_timestamp, latest_rewards_per_token) =
            self.latest_reward_per_token(ctx.storage)?;
        let total_farming_tokens = self.load_farming_totals(ctx.storage)?.farming;
        let total_lvn = emissions.lvn.raw();
        let elapsed_time = self
            .now()
            .checked_sub(latest_timestamp, "calculate_rewards_per_farming_token")?;
        let elapsed_time = Decimal256::from_ratio(elapsed_time.as_nanos(), 1u64);
        let elapsed_ratio = elapsed_time.checked_div(emissions_duration)?;
        let reward_per_token = total_lvn
            .checked_div_dec(total_farming_tokens.into_decimal256())?
            .checked_mul_dec(elapsed_ratio)?;
        let new_rewards_per_token = latest_rewards_per_token.checked_add(reward_per_token)?;

        Ok(new_rewards_per_token)
    }

    /// Calculates the amount of unlocked rewards are available since the last collection occurred
    pub(crate) fn calculate_unlocked_rewards(
        &self,
        farmer_stats: &RawFarmerStats,
        end_prefix_sum: LvnToken,
    ) -> Result<LvnToken> {
        let start_prefix_sum = farmer_stats.xlp_last_collected_prefix_sum;
        let unlocked_rewards = end_prefix_sum
            .checked_sub(start_prefix_sum)?
            .checked_mul_dec(farmer_stats.total_xlp()?.into_decimal256())?;

        Ok(unlocked_rewards)
    }

    /// Updates the rewards per farming token ratio since the last update
    pub(crate) fn update_rewards_per_token(&self, ctx: &mut StateContext) -> Result<()> {
        let rewards_per_token = self.calculate_rewards_per_token(ctx)?;
        REWARDS_PER_TIME_PER_TOKEN.save(ctx.storage, self.now(), &rewards_per_token)?;

        Ok(())
    }

    /// Calculates how many reward tokens the user can collect from LVN emissions
    /// and updates internal storage accordingly
    pub(crate) fn collect_lvn_emissions(
        &self,
        ctx: &mut StateContext,
        addr: &Addr,
    ) -> Result<LvnToken> {
        let mut farmer_stats = self.load_raw_farmer_stats(ctx.storage, addr)?;

        if farmer_stats.xlp_farming_tokens.is_zero() {
            return Ok(LvnToken::zero());
        }

        let end_prefix_sum = self.calculate_rewards_per_token(ctx)?;
        let unlocked_rewards = self.calculate_unlocked_rewards(&farmer_stats, end_prefix_sum)?;

        farmer_stats.xlp_last_collected_prefix_sum = end_prefix_sum;
        self.save_raw_farmer_stats(ctx, addr, &farmer_stats)?;

        Ok(unlocked_rewards)
    }
}
