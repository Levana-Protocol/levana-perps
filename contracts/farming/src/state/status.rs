use crate::prelude::*;

impl State<'_> {
    pub(crate) fn get_status(&self, store: &dyn Storage) -> Result<StatusResp> {
        let period = self.get_period(store)?;
        let farming_totals = self.get_farming_totals(store)?;
        let launched = self.get_launch_start(store).ok();

        Ok(StatusResp {
            period,
            farming_tokens: farming_totals.farming,
            xlp: farming_totals.xlp,
            launched,

            // TODO: add these
            lockdrop_buckets: Vec::new(),
            bonus: Collateral::zero(),
            lockdrop_rewards_unlocked: None,
            lvn_held: LvnToken::zero(),
            lvn_owed: LvnToken::zero(),
            emissions: None,
        })
    }
}
