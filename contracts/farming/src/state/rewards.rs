use cw_storage_plus::Item;
use msg::token::Token;
use crate::prelude::*;

/// The LVN token used for rewards
const LVN_TOKEN: Item<Token> = Item::new("lvn-token");

//todo don't forget to set LVN_LOCKDROP_REWARDS
/// The total amount of LVN rewards designated for lockdrop participants
const LVN_LOCKDROP_REWARDS: Item<LvnToken> = Item::new("lvn_lockdrop_rewards");

impl State<'_> {
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

    /// Calculates how many tokens the user can collect and updates internal storage accordingly
    pub(crate) fn collect_lockdrop_rewards(&self, ctx: &mut StateContext, farmer: &Addr) -> Result<LvnToken> {
        // First get the total amount of LVN tokens rewarded to this lockdrop participant

        let total_lockdrop_rewards = LVN_LOCKDROP_REWARDS.load(ctx.storage)?;
        let mut stats = self.load_raw_farmer_stats(ctx.storage, farmer)?; //todo should lockdrop stats be stored separately?
        let total_user_rewards = total_lockdrop_rewards
            .checked_mul_dec(stats.lockdrop_farming_tokens.into_decimal256())?;

        // Next, calculate how many tokens have unlocked

        //FIXME pull from config
        let lockdrop_start = Timestamp::from_seconds(0);
        //FIXME elapsed should be from last_claimed
        let elapsed_since_start = self.now().checked_sub(lockdrop_start, "claim_lockdrop_rewards")?;
        //FIXME pull from config
        let unlock_duration = Duration::from_seconds(60);

        let amount = if elapsed_since_start >= unlock_duration {
            total_user_rewards.checked_sub(stats.lockdrop_amount_collected)?
        } else {
            let elapsed_since_last_collected = (self.now().checked_sub(
                stats.lockdrop_last_collected,
            "claim_lockdrop_rewards, elapsed_since_last_collected"
            )?).as_nanos();

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

    pub(crate) fn collect_lvn_emissions(&self, ctx: &mut StateContext, farmer: &Addr) -> Result<LvnToken> {
        todo!()
    }
}