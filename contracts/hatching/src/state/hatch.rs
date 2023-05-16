use super::{
    get_lvn_to_grant,
    nft_mint::{get_nft_mint_iter, get_nft_mint_proxy_messages},
    State, StateContext,
};
use msg::contracts::hatching::{
    entry::PotentialHatchInfo,
    events::{HatchCompleteEvent, HatchRetryEvent, HatchStartEvent},
    HatchDetails, HatchStatus, NftBurnKind, NftHatchInfo,
};
use shared::{
    prelude::*,
    storage::{push_to_monotonic_map, MonotonicMap},
};

const HATCH_ID_BY_ADDR: Map<&Addr, u64> = Map::new("hatch-id-addr");
const HATCH_STATUS: MonotonicMap<HatchStatus> = Map::new("hatch-status");
const HATCH_DETAILS: Map<u64, HatchDetails> = Map::new("hatch-details");
const HATCH_ID_BY_TOKEN_ID: Map<&str, u64> = Map::new("hatch-id-token-id");

impl State<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hatch(
        &self,
        ctx: &mut StateContext,
        original_owner: Addr,
        nft_mint_owner: String,
        eggs: Vec<String>,
        dusts: Vec<String>,
        profile: bool,
        lvn_grant_address: String,
    ) -> Result<()> {
        if let Some(id) = HATCH_ID_BY_ADDR.may_load(ctx.storage, &original_owner)? {
            bail!("hatch already exists for {}, id: {}", original_owner, id);
        }

        let eggs = eggs
            .into_iter()
            .map(|token_id| self.burn_nft(ctx, &original_owner, NftBurnKind::Egg, token_id))
            .collect::<Result<Vec<NftHatchInfo>>>()?;

        let dusts = dusts
            .into_iter()
            .map(|token_id| self.burn_nft(ctx, &original_owner, NftBurnKind::Dust, token_id))
            .collect::<Result<Vec<NftHatchInfo>>>()?;

        let profile = if profile {
            self.drain_profile(ctx, &original_owner)?
        } else {
            None
        };

        let mut status = HatchStatus {
            nft_mint_completed: false,
            lvn_grant_completed: false,
            details: None,
        };

        let id = push_to_monotonic_map(ctx.storage, HATCH_STATUS, &status)?;

        let details = HatchDetails {
            original_owner: original_owner.clone(),
            nft_mint_owner,
            hatch_time: self.now(),
            eggs,
            dusts,
            profile,
            lvn_grant_address: lvn_grant_address.clone(),
        };

        let nfts_to_mint = get_nft_mint_proxy_messages(&details)?;
        if nfts_to_mint.0.is_empty() {
            // no nfts to mint, mark as completed
            status.nft_mint_completed = true;
        } else {
            for nft in get_nft_mint_iter(&details) {
                HATCH_ID_BY_TOKEN_ID.save(ctx.storage, &nft.token_id, &id)?;
            }
            self.send_mint_nfts_ibc_message(ctx, nfts_to_mint)?;
        }

        match get_lvn_to_grant(&details)? {
            None => {
                // no lvn to send, mark as completed
                status.lvn_grant_completed = true;
            }
            Some(amount) => {
                self.send_grant_lvn_ibc_message(ctx, id, lvn_grant_address, amount)?;
            }
        }

        ctx.response_mut().add_event(HatchStartEvent {
            id,
            details: details.clone(),
        });

        if status.nft_mint_completed && status.lvn_grant_completed {
            // this is unlikely to happen immediately, but not impossible. deal with it!
            HATCH_STATUS.remove(ctx.storage, id);
            HATCH_ID_BY_ADDR.remove(ctx.storage, &original_owner);
            ctx.response_mut()
                .add_event(HatchCompleteEvent { id, details });
        } else {
            // somewhat more likely, some of the requirements are finished, but not all
            // re-save the status with the updated values
            if status.nft_mint_completed || status.lvn_grant_completed {
                HATCH_STATUS.save(ctx.storage, id, &status)?;
            }

            // in either case, we have a proper hatching to track, save the lookups
            HATCH_DETAILS.save(ctx.storage, id, &details)?;
            HATCH_ID_BY_ADDR.save(ctx.storage, &original_owner, &id)?;
        }

        Ok(())
    }

    pub(crate) fn retry_hatch(&self, ctx: &mut StateContext, id: u64) -> Result<()> {
        let details = HATCH_DETAILS.load(ctx.storage, id)?;
        let status = HATCH_STATUS.load(ctx.storage, id)?;

        if !status.nft_mint_completed {
            self.send_mint_nfts_ibc_message(ctx, get_nft_mint_proxy_messages(&details)?)?;
        }

        if !status.lvn_grant_completed {
            let amount = get_lvn_to_grant(&details)?.context("re-granting 0 lvn")?;
            self.send_grant_lvn_ibc_message(ctx, id, details.lvn_grant_address.clone(), amount)?;
        }

        ctx.response_mut()
            .add_event(HatchRetryEvent { id, details });

        Ok(())
    }

    pub(crate) fn update_hatch_status(
        &self,
        ctx: &mut StateContext,
        id: u64,
        f: impl Fn(HatchStatus) -> Result<HatchStatus>,
    ) -> Result<()> {
        let status = f(HATCH_STATUS.load(ctx.storage, id)?)?;

        if status.lvn_grant_completed && status.nft_mint_completed {
            let details = HATCH_DETAILS.load(ctx.storage, id)?;

            HATCH_STATUS.remove(ctx.storage, id);
            HATCH_DETAILS.remove(ctx.storage, id);
            HATCH_ID_BY_ADDR.remove(ctx.storage, &details.original_owner);

            ctx.response_mut()
                .add_event(HatchCompleteEvent { id, details });
        } else {
            HATCH_STATUS.save(ctx.storage, id, &status)?;
        }

        Ok(())
    }

    pub(crate) fn get_oldest_hatch_status(
        &self,
        store: &dyn Storage,
        details: bool,
    ) -> Result<Option<(u64, HatchStatus)>> {
        // since hatch IDS are monotonically increasing, the first hatch in the map is the oldest
        // since we also remove completed hatches, this is the oldest *active* hatching
        match HATCH_STATUS
            .range(store, None, None, Order::Ascending)
            .next()
        {
            Some(resp) => {
                let (id, mut status) = resp?;
                if details {
                    status.details = Some(HATCH_DETAILS.load(store, id)?);
                }
                Ok(Some((id, status)))
            }
            None => Ok(None),
        }
    }
    pub(crate) fn get_hatch_status_by_id(
        &self,
        store: &dyn Storage,
        id: u64,
        details: bool,
    ) -> Result<Option<(u64, HatchStatus)>> {
        match HATCH_STATUS.may_load(store, id)? {
            Some(mut status) => {
                if details {
                    status.details = Some(HATCH_DETAILS.load(store, id)?);
                }
                Ok(Some((id, status)))
            }
            None => Ok(None),
        }
    }

    pub(crate) fn get_hatch_status_by_owner(
        &self,
        store: &dyn Storage,
        owner: &Addr,
        details: bool,
    ) -> Result<Option<(u64, HatchStatus)>> {
        HATCH_ID_BY_ADDR
            .may_load(store, owner)?
            .and_then(|id| self.get_hatch_status_by_id(store, id, details).transpose())
            .transpose()
    }

    pub(crate) fn get_hatch_id_from_token_id(
        &self,
        store: &dyn Storage,
        token_id: &str,
    ) -> Result<u64> {
        HATCH_ID_BY_TOKEN_ID
            .load(store, token_id)
            .map_err(|err| err.into())
    }

    pub(crate) fn get_potential_hatch_info(
        &self,
        owner: &Addr,
        eggs: Vec<String>,
        dusts: Vec<String>,
        profile: bool,
    ) -> Result<PotentialHatchInfo> {
        let eggs = eggs
            .into_iter()
            .map(|token_id| self.get_nft_info(owner, NftBurnKind::Egg, token_id))
            .collect::<Result<Vec<NftHatchInfo>>>()?;

        let dusts = dusts
            .into_iter()
            .map(|token_id| self.get_nft_info(owner, NftBurnKind::Dust, token_id))
            .collect::<Result<Vec<NftHatchInfo>>>()?;

        let profile = if profile {
            self.get_profile_info(owner)?
        } else {
            None
        };

        Ok(PotentialHatchInfo {
            eggs,
            dusts,
            profile,
        })
    }
}
