use perpswap::contracts::{
    factory::entry::{make_referee_count_key, make_referrer_key},
    market::{entry::ReferralStatsResp, position::CollateralAndUsd},
};

use crate::prelude::*;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct ReferralStats {
    generated: CollateralAndUsd,
    received: CollateralAndUsd,
}

const REFERRAL_STATS_MAP: Map<&Addr, ReferralStats> = Map::new(namespace::REFERRAL_STATS_MAP);

impl State<'_> {
    pub(crate) fn add_summary_referral(
        &self,
        ctx: &mut StateContext,
        referee: &Addr,
        referrer: &Addr,
        referral_earned: NonZero<Collateral>,
    ) -> Result<()> {
        let price = self.current_spot_price(ctx.storage)?;

        let mut referee_stats = REFERRAL_STATS_MAP
            .may_load(ctx.storage, referee)?
            .unwrap_or_default();
        referee_stats
            .generated
            .checked_add_assign(referral_earned.raw(), &price)?;
        REFERRAL_STATS_MAP.save(ctx.storage, referee, &referee_stats)?;

        let mut referrer_stats = REFERRAL_STATS_MAP
            .may_load(ctx.storage, referrer)?
            .unwrap_or_default();
        referrer_stats
            .received
            .checked_add_assign(referral_earned.raw(), &price)?;
        REFERRAL_STATS_MAP.save(ctx.storage, referrer, &referrer_stats)?;

        ctx.response_mut().add_event(
            Event::new("referral-reward")
                .add_attribute("referrer", referrer)
                .add_attribute("referee", referee)
                .add_attribute("collateral", referral_earned.to_string())
                .add_attribute(
                    "usd",
                    price.collateral_to_usd(referral_earned.raw()).to_string(),
                ),
        );

        Ok(())
    }

    pub(crate) fn get_referrer_for(&self, referee: &Addr) -> Result<Option<Addr>> {
        match self
            .querier
            .query_wasm_raw(&self.factory_address, make_referrer_key(referee).as_bytes())?
        {
            None => Ok(None),
            Some(referrer) => RawAddr::from(String::from_utf8(referrer)?)
                .validate(self.api)
                .map(Some),
        }
    }

    pub(crate) fn referral_stats(
        &self,
        store: &dyn Storage,
        addr: &Addr,
    ) -> Result<ReferralStatsResp> {
        let key = make_referee_count_key(addr);
        let referees = match self
            .querier
            .query_wasm_raw(&self.factory_address, key.as_bytes())?
        {
            Some(referees) => {
                let referees = String::from_utf8(referees)?;
                referees.parse()?
            }
            None => 0,
        };
        let ReferralStats {
            generated,
            received,
        } = REFERRAL_STATS_MAP
            .may_load(store, addr)?
            .unwrap_or_default();
        Ok(ReferralStatsResp {
            generated: generated.collateral(),
            generated_usd: generated.usd(),
            received: received.collateral(),
            received_usd: received.usd(),
            referees,
            referrer: self.get_referrer_for(addr)?,
        })
    }
}
