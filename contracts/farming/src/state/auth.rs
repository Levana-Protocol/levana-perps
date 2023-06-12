use crate::prelude::*;

const OWNER: Item<Addr> = Item::new("owner");

impl State<'_> {
    pub(crate) fn set_owner(&self, ctx: &mut StateContext, owner: &Addr) -> Result<()> {
        OWNER.save(ctx.storage, owner).map_err(|err| err.into())
    }

    pub(crate) fn validate_owner(&self, store: &dyn Storage, wallet: &Addr) -> Result<()> {
        OWNER
            .load(store)
            .map_err(|err| err.into())
            .and_then(|owner| {
                if owner == wallet {
                    Ok(())
                } else {
                    perp_bail!(ErrorId::Auth, ErrorDomain::Farming, "Unauthorized")
                }
            })
    }
}
