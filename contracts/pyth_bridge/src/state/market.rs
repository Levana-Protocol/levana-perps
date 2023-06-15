use crate::state::*;
use msg::contracts::{
    market::entry::ExecuteMsg as MarketExecuteMsg,
    pyth_bridge::{events::UpdatePriceEvent, MarketPrice, PythMarketPriceFeeds},
};
use msg::prelude::*;

const PYTH_PREV_MARKET_PRICE: Map<&MarketId, MarketPrice> =
    Map::new(namespace::PYTH_PREV_MARKET_PRICE);

impl State<'_> {
    pub(crate) fn market_addr(&self, market_id: MarketId) -> Result<Addr> {
        load_external_map(
            &self.querier,
            &self.factory_address,
            namespace::MARKET_ADDRS,
            &market_id,
        )
    }

    pub(crate) fn market_price(
        &self,
        store: &dyn Storage,
        market_id: &MarketId,
        age_tolerance_seconds: u64,
    ) -> Result<MarketPrice> {
        let PythMarketPriceFeeds { feeds, feeds_usd } =
            self.get_pyth_market_price_feeds(store, market_id)?;
        let (price, publish_time) = self.get_pyth_price(store, feeds, age_tolerance_seconds)?;
        let price_usd = feeds_usd
            .map(|feeds_usd| self.get_pyth_price(store, feeds_usd, age_tolerance_seconds))
            .transpose()?;

        Ok(MarketPrice {
            price: PriceBaseInQuote::from_non_zero(price),
            price_usd: price_usd
                .map(|(price_usd, _)| PriceCollateralInUsd::from_non_zero(price_usd)),
            latest_price_publish_time: publish_time,
            latest_price_usd_publish_time: price_usd.map(|(_, publish_time)| publish_time),
        })
    }

    pub(crate) fn update_market_price(
        &self,
        ctx: &mut StateContext,
        market_id: MarketId,
        execs: Option<u32>,
        reward_addr: RawAddr,
    ) -> Result<()> {
        // for updating market price, it must be set within the configured age tolerance
        let market_price = self.market_price(
            ctx.storage,
            &market_id,
            self.get_pyth_update_age_tolerance(ctx.storage)?,
        )?;

        // if we have a previous price point, we check that the new price point is newer
        if let Some(prev_market_price) = PYTH_PREV_MARKET_PRICE.may_load(ctx.storage, &market_id)? {
            let prev_price_valid = market_price.latest_price_publish_time
                > prev_market_price.latest_price_publish_time;

            let prev_price_usd_valid = match (
                prev_market_price.latest_price_usd_publish_time,
                market_price.latest_price_usd_publish_time,
            ) {
                (None, Some(_)) | (Some(_), None) => {
                    // if we ever hit this error, it probably means we should have done a contract migration or re-deploy
                    // since the existence of a price_usd feed is determined at market instantiation time
                    perp_bail!(
                        ErrorId::PriceNotFound,
                        ErrorDomain::Pyth,
                        "Price USD changed from None to Some or vice versa"
                    );
                }
                // if we have no usd feed, then validity is the same as the regular price feed
                (None, None) => prev_price_valid,
                // otherwise, we check the publish time
                (Some(prev), Some(curr)) => curr > prev,
            };

            if !prev_price_valid || !prev_price_usd_valid {
                perp_bail!(
                    ErrorId::PriceAlreadyExists,
                    ErrorDomain::Pyth,
                    "No new price data"
                );
            }
        }

        PYTH_PREV_MARKET_PRICE.save(ctx.storage, &market_id, &market_price)?;

        let MarketPrice {
            price, price_usd, ..
        } = market_price;

        ctx.response.add_event(UpdatePriceEvent {
            market_id: market_id.clone(),
            price,
            price_usd,
        });

        ctx.response.add_execute_submessage_oneshot(
            self.market_addr(market_id)?,
            &MarketExecuteMsg::SetPrice {
                price,
                price_usd,
                execs,
                rewards: Some(reward_addr),
            },
        )
    }
}
