use crate::prelude::*;
use crate::prelude::rewards::BonusConfig;

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
        let emissions = self.may_load_lvn_emissions(store)?;
        let bonus_config = self.load_bonus_config(store)?;

        Ok(StatusResp {
            period,
            farming_tokens: farming_totals.farming,
            xlp: farming_totals.xlp,
            bonus,
            bonus_ratio: bonus_config.ratio,
            bonus_addr: bonus_config.addr,
            lockdrop_buckets,
            lockdrop_rewards_unlocked,
            emissions,
        })
    }
}
