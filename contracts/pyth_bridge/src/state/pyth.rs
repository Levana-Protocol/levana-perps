use crate::state::*;
use msg::{
    contracts::pyth_bridge::{
        events::SetMarketPriceFeedsEvent, AllPythMarketPriceFeeds, PythMarketPriceFeeds,
        PythPriceFeed,
    },
    prelude::*,
};
use pyth_sdk_cw::{PriceFeedResponse, UnixTimestamp};

const PYTH_ADDR: Item<Addr> = Item::new(namespace::PYTH_ADDR);
const PYTH_UPDATE_AGE_TOLERANCE: Item<u64> = Item::new(namespace::PYTH_UPDATE_AGE_TOLERANCE);
const PYTH_MARKET_PRICE_FEEDS: Map<&MarketId, PythMarketPriceFeeds> =
    Map::new(namespace::PYTH_MARKET_PRICE_FEEDS);

pub(crate) fn set_pyth_addr(store: &mut dyn Storage, addr: &Addr) -> Result<()> {
    PYTH_ADDR.save(store, addr).map_err(|err| err.into())
}

impl State<'_> {
    pub(crate) fn get_pyth_addr(&self, store: &dyn Storage) -> Result<Addr> {
        PYTH_ADDR.load(store).map_err(|err| err.into())
    }
    pub(crate) fn get_pyth_update_age_tolerance(&self, store: &dyn Storage) -> Result<u64> {
        PYTH_UPDATE_AGE_TOLERANCE
            .load(store)
            .map_err(|err| err.into())
    }

    pub(crate) fn get_all_pyth_market_price_feeds(
        &self,
        store: &dyn Storage,
        start_after: Option<&MarketId>,
        limit: Option<u32>,
        order: Option<Order>,
    ) -> Result<AllPythMarketPriceFeeds> {
        const MAX_LIMIT: u32 = 20;
        let limit: usize = limit.unwrap_or(MAX_LIMIT).min(MAX_LIMIT).try_into()?;

        let markets = PYTH_MARKET_PRICE_FEEDS
            .range(
                store,
                start_after.map(Bound::exclusive),
                None,
                order.unwrap_or(Order::Ascending),
            )
            .take(limit)
            .map(|res| res.map_err(|err| err.into()))
            .collect::<Result<Vec<_>>>()?;

        Ok(AllPythMarketPriceFeeds { markets })
    }

    pub(crate) fn get_pyth_market_price_feeds(
        &self,
        store: &dyn Storage,
        market_id: &MarketId,
    ) -> Result<PythMarketPriceFeeds> {
        PYTH_MARKET_PRICE_FEEDS
            .load(store, market_id)
            .map_err(|err| err.into())
    }

    // adapted from: https://github.com/pyth-network/pyth-crosschain/blob/ed37358da297f24df604e31523dff3ddcbf847fa/target_chains/cosmwasm/examples/cw-contract/src/contract.rs#L85
    pub(crate) fn get_pyth_price(
        &self,
        store: &dyn Storage,
        feeds: Vec<PythPriceFeed>,
        age_tolerance_seconds: u64,
    ) -> Result<(NumberGtZero, UnixTimestamp)> {
        let pyth_addr = self.get_pyth_addr(store)?;

        let mut acc_price: Option<(Number, UnixTimestamp)> = None;

        for PythPriceFeed { id, inverted } in feeds {
            let price_feed_response: PriceFeedResponse =
                pyth_sdk_cw::query_price_feed(&self.querier, pyth_addr.clone(), id)?;
            let price_feed = price_feed_response.price_feed;

            let current_block_time_seconds = self.env.block.time.seconds().try_into()?;
            let price = price_feed
                // alternative: .get_emaprice_no_older_than()
                .get_price_no_older_than(
                    current_block_time_seconds,
                    age_tolerance_seconds,
                )
                .ok_or_else(|| {
                    perp_error!(
                        ErrorId::PriceTooOld,
                        ErrorDomain::Pyth,
                        "Current price is not available. Price id: {}, inverted: {}, Current block time: {}, price publish time: {}, diff: {}, age_tolerance: {}",
                        id,
                        inverted,
                        current_block_time_seconds,
                        price_feed.get_price_unchecked().publish_time,
                        (price_feed.get_price_unchecked().publish_time - current_block_time_seconds).abs(),
                        age_tolerance_seconds
                    )
                })?;

            let publish_time = price.publish_time;
            let price: Number = Number::try_from(price)?;

            acc_price = match acc_price {
                None => Some((price, publish_time)),
                Some((prev_price, prev_publish_time)) => {
                    let publish_time = publish_time.max(prev_publish_time);
                    let next_price =
                        compose_price(prev_price.into_number(), price.into_number(), inverted)?;
                    Some((next_price, publish_time))
                }
            }
        }

        match acc_price {
            Some((price, publish_time)) => {
                let price = NumberGtZero::try_from(price)?;
                Ok((price, publish_time))
            }
            None => anyhow::bail!("No price feeds provided"),
        }
    }

    pub(crate) fn set_pyth_market_price_feeds(
        &self,
        ctx: &mut StateContext,
        market_id: MarketId,
        market_price_feeds: PythMarketPriceFeeds,
    ) -> Result<()> {
        PYTH_MARKET_PRICE_FEEDS.save(ctx.storage, &market_id, &market_price_feeds)?;

        ctx.response.add_event(SetMarketPriceFeedsEvent {
            market_id,
            market_price_feeds,
        });

        Ok(())
    }

    pub(crate) fn set_pyth_update_age_tolerance(
        &self,
        ctx: &mut StateContext,
        tolerance_seconds: u64,
    ) -> Result<()> {
        PYTH_UPDATE_AGE_TOLERANCE.save(ctx.storage, &tolerance_seconds)?;

        Ok(())
    }
}

fn compose_price(prev: Number, mut curr: Number, curr_inverted: bool) -> Result<Number> {
    if curr_inverted {
        curr = Number::ONE / curr;
    }

    Ok(prev * curr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyth_route_compose() {
        let eth_usd = pyth_sdk_cw::Price {
            price: 179276800001,
            conf: 0,
            expo: -8,
            publish_time: 0,
        };

        let btc_usd = pyth_sdk_cw::Price {
            price: 2856631500000,
            conf: 0,
            expo: -8,
            publish_time: 0,
        };

        let eth_btc = compose_price(
            eth_usd.try_into().unwrap(),
            btc_usd.try_into().unwrap(),
            true,
        )
        .unwrap();

        assert_eq!(eth_btc, Number::try_from("0.062758112133468261").unwrap());
    }
}
