use perpswap::{contracts::faucet::entry::FaucetAsset, prelude::*};

use super::{State, StateContext};

type History<'a> = Map<(&'a str, Timestamp), Decimal256>;

const HISTORY_CW20: History = Map::new("HISTORY_CW20");
const HISTORY_NATIVE: History = Map::new("HISTORY_NATIVE");

fn get_history_map_for_asset(asset: &FaucetAsset) -> (History, &str) {
    match asset {
        FaucetAsset::Cw20(x) => (HISTORY_CW20, x.as_str()),
        FaucetAsset::Native(x) => (HISTORY_NATIVE, x),
    }
}

impl State<'_> {
    /// Returns the cumulative distributed up until and including this timestamp.
    ///
    /// Returns 0 if nothing has been distributed.
    pub(crate) fn get_history(
        &self,
        store: &dyn Storage,
        asset: &FaucetAsset,
        timestamp: Timestamp,
    ) -> Result<Decimal256> {
        let (map, key) = get_history_map_for_asset(asset);
        match map
            .prefix(key)
            .range(
                store,
                None,
                Some(Bound::inclusive(timestamp)),
                Order::Descending,
            )
            .next()
        {
            Some(res) => {
                let (_, amount) = res?;
                Ok(amount)
            }
            None => Ok(Decimal256::zero()),
        }
    }

    pub(crate) fn add_history(
        &self,
        ctx: &mut StateContext,
        asset: &FaucetAsset,
        amount: Decimal256,
    ) -> Result<()> {
        // Get the most recent value. Include the current timestamp, so that if
        // multiple actions happen in a single block we include them all.
        let now = self.now();
        let old_amount = self.get_history(ctx.storage, asset, now)?;
        let new_amount = amount
            .checked_add(old_amount)
            .context("Overflow when calculating total gas in add_history")?;
        let (map, key) = get_history_map_for_asset(asset);
        map.save(ctx.storage, (key, now), &new_amount)?;
        Ok(())
    }
}
