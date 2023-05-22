use crate::prelude::*;

impl State<'_> {
    pub(crate) fn get_status(&self, store: &dyn Storage) -> Result<StatusResp> {
        let period = self.get_period_resp(store)?;
        let farming_totals = self.load_farming_totals(store)?;
        let emissions = self.may_load_lvn_emissions(store)?;

        Ok(StatusResp {
            period,
            farming_tokens: farming_totals.farming,
            xlp: farming_totals.xlp,
            emissions,

            // TODO: add these
            lockdrop_buckets: Vec::new(),
            bonus: Collateral::zero(),
            lockdrop_rewards_unlocked: None,
            lvn_held: LvnToken::zero(),
            lvn_owed: LvnToken::zero(),
        })
    }
}
