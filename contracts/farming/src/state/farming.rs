use crate::prelude::*;

/// Total number of farming tokens.
const TOTALS: Item<FarmingTotals> = Item::new("farming-totals");

/// Farming stats per wallet.
const FARMERS: Map<&Addr, RawFarmerStats> = Map::new("farmer-stats");

#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub(crate) struct RawFarmerStats {
    /// The amount of farming tokens owned by this farmer
    pub(crate) farming_tokens: FarmingToken,
    /// A timestamp representing the last time the farmer collected lockdrop rewards
    pub(crate) lockdrop_last_collected: Option<Timestamp>,
    /// The amount of LVN tokens collected by the farmer up until [lockdrop_last_collected]
    pub(crate) lockdrop_amount_collected: LvnToken,
    /// The prefix sum of the last time the farmer claimed.
    /// See [REWARDS_PER_TIME_PER_TOKEN] for more explanation of prefix sums.
    pub(crate) xlp_last_claimed_prefix_sum: LvnToken,
    /// The amount of LVN tokens that have accrued from emissions but have not yet been claimed
    pub(crate) accrued_emissions: LvnToken,
}

#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub(crate) struct FarmingTotals {
    /// Total amount of xLP controlled by this contract.
    ///
    /// We could in theory query the xLP balance instead of storing it. However,
    /// that gets hairy quickly when users deposit xLP, since querying the xLP
    /// token balance will not give the correct response for calculations.
    pub xlp: LpToken,
    pub farming: FarmingToken,
}

impl FarmingTotals {
    fn xlp_to_farming(&self, xlp: LpToken) -> Result<FarmingToken> {
        anyhow::ensure!(
            self.farming.is_zero() == self.xlp.is_zero(),
            "We must either have no farming and no xLP tokens, or have some of both"
        );
        Ok(if self.farming.is_zero() {
            FarmingToken::from_decimal256(xlp.into_decimal256())
        } else {
            FarmingToken::from_decimal256(
                self.farming
                    .into_decimal256()
                    .checked_mul(xlp.into_decimal256())?
                    .checked_div(self.xlp.into_decimal256())?,
            )
        })
    }

    fn farming_to_xlp(&self, farming: FarmingToken) -> Result<LpToken> {
        anyhow::ensure!(!self.farming.is_zero());
        anyhow::ensure!(!self.xlp.is_zero());

        Ok(LpToken::from_decimal256(
            self.xlp
                .into_decimal256()
                .checked_mul(farming.into_decimal256())?
                .checked_div(self.farming.into_decimal256())?,
        ))
    }
}

impl State<'_> {
    /// Get the total amount of xLP held by this contract
    pub(crate) fn load_farming_totals(&self, store: &dyn Storage) -> Result<FarmingTotals> {
        TOTALS
            .may_load(store)
            .map_err(|e| e.into())
            .map(|x| x.unwrap_or_default())
    }

    pub(crate) fn save_farming_totals(
        &self,
        store: &mut dyn Storage,
        totals: &FarmingTotals,
    ) -> Result<()> {
        TOTALS.save(store, totals)?;
        Ok(())
    }

    /// Load the raw farmer stats for the given farmer.
    /// If the farmer does not exist, returns a default [RawFarmerStats]
    pub(crate) fn load_raw_farmer_stats(
        &self,
        store: &dyn Storage,
        farmer: &Addr,
    ) -> Result<Option<RawFarmerStats>> {
        FARMERS
            .may_load(store, farmer)
            .map_err(|e| e.into())
    }

    /// Save the raw farmer stats for the given farmer.
    pub(crate) fn save_raw_farmer_stats(
        &self,
        store: &mut dyn Storage,
        farmer: &Addr,
        raw: &RawFarmerStats,
    ) -> Result<()> {
        FARMERS.save(store, farmer, raw).map_err(|e| e.into())
    }

    /// Update internal farming token balances to represent a deposit of xLP for the given farmer.
    pub(crate) fn farming_deposit(
        &self,
        ctx: &mut StateContext,
        farmer: &Addr,
        xlp: LpToken,
    ) -> Result<FarmingToken> {
        let mut farmer_stats = match self.load_raw_farmer_stats(ctx.storage, farmer)? {
            None => RawFarmerStats::default(),
            Some(farmer_stats) => farmer_stats
        };

        self.farming_perform_emissions_bookkeeping(ctx, farmer, &mut farmer_stats)?;

        let mut totals = self.load_farming_totals(ctx.storage)?;
        let new_farming = totals.xlp_to_farming(xlp)?;

        totals.xlp = totals.xlp.checked_add(xlp)?;
        totals.farming = totals.farming.checked_add(new_farming)?;
        self.save_farming_totals(ctx.storage, &totals)?;

        farmer_stats.farming_tokens = farmer_stats.farming_tokens.checked_add(new_farming)?;
        self.save_raw_farmer_stats(ctx.storage, farmer, &farmer_stats)?;

        Ok(new_farming)
    }

    /// Update internal farming token balances to indicate a withdrawal of the given number of farming tokens.
    ///
    /// Returns the amount of xLP tokens that were withdrawn.
    pub(crate) fn farming_withdraw(
        &self,
        ctx: &mut StateContext,
        farmer: &Addr,
        amount: Option<NonZero<FarmingToken>>,
    ) -> Result<(LpToken, FarmingToken)> {
        let mut farmer_stats = match self.load_raw_farmer_stats(ctx.storage, farmer)? {
            None => bail!("Unable to withdraw, {} does not exist", farmer),
            Some(farmer_stats) => farmer_stats
        };

        self.farming_perform_emissions_bookkeeping(ctx, farmer, &mut farmer_stats)?;

        let mut totals = self.load_farming_totals(ctx.storage)?;

        let amount = match amount {
            Some(amount) => amount.raw(),
            None => farmer_stats.farming_tokens,
        };

        anyhow::ensure!(
            amount <= farmer_stats.farming_tokens,
            "Insufficient farming tokens. Wanted: {amount}. Available: {}.",
            farmer_stats.farming_tokens
        );
        anyhow::ensure!(!amount.is_zero(), "Cannot withdraw 0 farming tokens");

        let removed_xlp = totals.farming_to_xlp(amount)?;

        totals.farming = totals.farming.checked_sub(amount)?;
        totals.xlp = totals.xlp.checked_sub(removed_xlp)?;
        self.save_farming_totals(ctx.storage, &totals)?;

        farmer_stats.farming_tokens = farmer_stats.farming_tokens.checked_sub(amount)?;
        self.save_raw_farmer_stats(ctx.storage, farmer, &farmer_stats)?;

        Ok((removed_xlp, amount))
    }
}
