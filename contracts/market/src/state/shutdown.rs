use anyhow::Result;
use cosmwasm_std::Storage;
use cw_storage_plus::Item;
use perpswap::shutdown::ShutdownImpact;
use perpswap::{namespace, prelude::external_map_has};

use super::{State, StateContext};

const CLOSE_ALL_POSITIONS: Item<()> = Item::new(namespace::CLOSE_ALL_POSITIONS);

impl State<'_> {
    /// Start closing all positions via the crank
    pub(crate) fn set_close_all_positions(&self, ctx: &mut StateContext) -> Result<()> {
        anyhow::ensure!(
            CLOSE_ALL_POSITIONS.may_load(ctx.storage)?.is_none(),
            "Already closing all positions"
        );
        CLOSE_ALL_POSITIONS.save(ctx.storage, &())?;
        Ok(())
    }

    /// Are we already closing all positions?
    pub(crate) fn get_close_all_positions(&self, store: &dyn Storage) -> Result<bool> {
        CLOSE_ALL_POSITIONS
            .may_load(store)
            .map(|x| x.is_some())
            .map_err(|e| e.into())
    }

    pub(crate) fn ensure_not_shut_down(&self, impact: ShutdownImpact) -> Result<()> {
        let is_disabled = external_map_has(
            &self.querier,
            &self.factory_address,
            namespace::SHUTDOWNS,
            &(&self.env.contract.address, impact),
        )?;
        if is_disabled {
            Err(anyhow::anyhow!(
                "Cannot perform action, market shutdown in place for {impact:?}"
            ))
        } else {
            Ok(())
        }
    }
}
