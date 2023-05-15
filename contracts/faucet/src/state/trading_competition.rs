use crate::state::*;
use msg::contracts::faucet::entry::FaucetAsset;

use super::tokens::TOKEN_INFO;

/// Key is a pair of the wallet address and CW20 contract address.
const TAPPED_ONCE: Map<(&Addr, &Addr), ()> = Map::new(namespace::TC_TAPPED_ONCE);

pub(crate) enum AlreadyTappedTradingCompetition {
    NotTradingCompetition,
    AlreadyTapped(Addr),
    DidNotTap(Addr),
}

impl State<'_> {
    /// Return a CW20 address for the asset if it's a trading competition address.
    fn get_trading_competition_cw20(
        &self,
        store: &dyn Storage,
        asset: &FaucetAsset,
    ) -> Result<Option<Addr>> {
        let cw20 = match asset {
            FaucetAsset::Cw20(addr) => addr,
            FaucetAsset::Native(_) => return Ok(None),
        };
        let cw20 = cw20.validate(self.api)?;
        let token_info = match TOKEN_INFO.may_load(store, &cw20)? {
            Some(token_info) => token_info,
            None => return Ok(None),
        };
        Ok(token_info.trading_competition_index.map(|_| cw20))
    }

    pub(crate) fn already_tapped_trading_competition(
        &self,
        store: &dyn Storage,
        recipient: &Addr,
        asset: &FaucetAsset,
    ) -> Result<AlreadyTappedTradingCompetition> {
        let cw20 = match self.get_trading_competition_cw20(store, asset)? {
            Some(cw20) => cw20,
            None => return Ok(AlreadyTappedTradingCompetition::NotTradingCompetition),
        };
        Ok(if TAPPED_ONCE.has(store, (recipient, &cw20)) {
            AlreadyTappedTradingCompetition::AlreadyTapped(cw20)
        } else {
            AlreadyTappedTradingCompetition::DidNotTap(cw20)
        })
    }

    pub(crate) fn set_tapped_trading_competition(
        &self,
        ctx: &mut StateContext,
        recipient: &Addr,
        asset: &FaucetAsset,
    ) -> Result<()> {
        if let Some(cw20) = self.get_trading_competition_cw20(ctx.storage, asset)? {
            TAPPED_ONCE.save(ctx.storage, (recipient, &cw20), &())?;
        }
        Ok(())
    }

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
