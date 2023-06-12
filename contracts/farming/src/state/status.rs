use crate::prelude::*;

impl State<'_> {
    pub(crate) fn get_status(&self, store: &dyn Storage) -> Result<StatusResp> {
        let period = self.get_period_resp(store)?;
        let farming_totals = self.load_farming_totals(store)?;
        let bonus = self.load_bonus_fund(store)?;
        let lockdrop_buckets = self.lockdrop_bucket_stats(store)?;
        let lockdrop_rewards_unlocked = match period {
            FarmingPeriodResp::Launched { started_at } => {
                let unlock_duration = self
                    .load_lockdrop_config(store)?
                    .lockdrop_lvn_unlock_seconds;
                Some(started_at + unlock_duration)
            }
            _ => None,
        };
        let lvn_held = self
            .load_lvn_token(store)?
            .query_balance_dec(&self.querier, &self.env.contract.address)
            .map(LvnToken::from_decimal256)?;
        let lvn_owed = LvnToken::zero(); //TODO fill this in
        let emissions = self.may_load_lvn_emissions(store)?;

        Ok(StatusResp {
            period,
            farming_tokens: farming_totals.farming,
            xlp: farming_totals.xlp,
            bonus,
            lockdrop_buckets,
            lockdrop_rewards_unlocked,
            lvn_held,
            lvn_owed,
            emissions,
        })
    }
}
