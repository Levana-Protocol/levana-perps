use crate::state::*;
use msg::contracts::faucet::entry::FaucetAsset;

use super::tokens::TOKEN_INFO;

/// Key is a pair of the wallet address and CW20 contract address.
const TAPPED_ONCE: Map<(&Addr, &Addr), ()> = Map::new(namespace::TC_TAPPED_ONCE);

impl State<'_> {
    pub(crate) fn assert_trading_competition(
        &self,
        ctx: &mut StateContext,
        recipient: Addr,
        asset: &FaucetAsset,
    ) -> Result<()> {
        let cw20 = match asset {
            FaucetAsset::Cw20(cw20) => cw20.validate(self.api)?,
            FaucetAsset::Native(_) => return Ok(()),
        };

        let token_info = match TOKEN_INFO.may_load(ctx.storage, &cw20)? {
            Some(x) => x,
            // Could arguably just exit saying we don't support this token...
            None => return Ok(()),
        };

        if token_info.trading_competition_index.is_none() {
            return Ok(());
        }

        if TAPPED_ONCE.has(ctx.storage, (&cw20, &recipient)) {
            Err(perp_anyhow!(
                ErrorId::Auth,
                ErrorDomain::Faucet,
                "failed auth, cw20 {cw20}, recipient: {:?}, reason: trading competition",
                recipient,
            ))
        } else {
            TAPPED_ONCE.save(ctx.storage, (&cw20, &recipient), &())?;
            Ok(())
        }
    }
}
