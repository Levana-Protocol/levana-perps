use crate::prelude::*;

const ADMIN:Item<Addr> = Item::new("admin");

impl State<'_> {
    pub fn set_admin(&self, ctx: &mut StateContext, admin: &Addr) -> Result<()> {
        ADMIN.save(ctx.storage, admin).map_err(|err| err.into())
    }

    pub fn validate_admin(&self, store: &dyn Storage, wallet: &Addr) -> Result<()> {
        ADMIN
            .load(store)
            .map_err(|err| err.into())
            .and_then(|admin| {
                if admin == wallet {
                    Ok(())
                } else {
                    perp_bail!(ErrorId::Auth, ErrorDomain::Farming, "Unauthorized")
                }
            })
    }
}