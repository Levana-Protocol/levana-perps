use crate::state::*;
use cosmwasm_std::{wasm_execute, SubMsg, SubMsgResponse};
use msg::contracts::{
    market::entry::ExecuteMsg as MarketExecuteMsg,
    pyth_bridge::{events::UpdatePriceEvent, MarketPrice, PythMarketPriceFeeds},
};
use msg::prelude::*;
use serde::{Deserialize, Serialize};

const PYTH_PREV_MARKET_PRICE: Map<&MarketId, MarketPrice> =
    Map::new(namespace::PYTH_PREV_MARKET_PRICE);

const LAST_BLOCK_TIME_UPDATED: Item<Timestamp> = Item::new(namespace::PYTH_LAST_BLOCK_TIME_UPDATED);

const REPLY_CONTEXT: Item<ReplyContext> = Item::new(namespace::PYTH_REPLY_CONTEXT);

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ReplyContext {
    pub market_price: MarketPrice,
    pub market_id: MarketId,
    pub bail_on_error: bool,
}

impl ReplyContext {
    pub const ID: u64 = 1;
}
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

        // for updating market price, it must be set within the configured age tolerance
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
        bail_on_error: bool,
    ) -> Result<()> {
        // In order to make gas simulation more accurate, always attempt an update
        // then erroring out (or not) is handled in the reply handler
        let market_price = self.market_price(
            ctx.storage,
            &market_id,
            self.get_pyth_update_age_tolerance(ctx.storage)?,
        )?;

        let MarketPrice {
            price, price_usd, ..
        } = market_price;

        ctx.response.add_event(UpdatePriceEvent {
            market_id: market_id.clone(),
            price,
            price_usd,
        });

        REPLY_CONTEXT.save(
            ctx.storage,
            &ReplyContext {
                market_price,
                market_id: market_id.clone(),
                bail_on_error,
            },
        )?;

        ctx.response.add_raw_submessage(SubMsg::reply_always(
            wasm_execute(
                self.market_addr(market_id)?,
                &MarketExecuteMsg::SetPrice {
                    price,
                    price_usd,
                    execs,
                    rewards: Some(reward_addr),
                },
                vec![],
            )?,
            ReplyContext::ID,
        ));

        Ok(())
    }

    pub(crate) fn handle_reply(
        &self,
        ctx: &mut StateContext,
        reply_result: Result<SubMsgResponse, String>,
    ) -> Result<()> {
        let ReplyContext {
            market_price,
            market_id,
            bail_on_error,
        } = REPLY_CONTEXT.load(ctx.storage)?;

        let mut err: Option<anyhow::Error> = None;
        let mut set_err = |e: anyhow::Error| {
            // while we wait until the end to keep gas simulations consistent
            // in theory we would have bailed with the first error
            if err.is_none() {
                err = Some(e);
            }
        };

        // Pyth may have updated its time from another transaction in the same block
        // so the pyth time may have moved forward since our last update
        // but we should still only allow one update per block
        if let Some(prev_block_time) = LAST_BLOCK_TIME_UPDATED.may_load(ctx.storage)? {
            match self.now().cmp(&prev_block_time) {
                std::cmp::Ordering::Less => {
                    set_err(perp_anyhow!(
                        ErrorId::TimestampSubtractUnderflow,
                        ErrorDomain::Pyth,
                        "Time must always move forward"
                    ));
                }
                std::cmp::Ordering::Equal => {
                    set_err(perp_anyhow!(
                        ErrorId::PriceAlreadyExists,
                        ErrorDomain::Pyth,
                        "Price already updated"
                    ));
                }
                std::cmp::Ordering::Greater => {}
            }
        }

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
                    set_err(perp_anyhow!(
                        ErrorId::PriceNotFound,
                        ErrorDomain::Pyth,
                        "Price USD changed from None to Some or vice versa"
                    ));
                    false
                }
                // if we have no usd feed, then validity is the same as the regular price feed
                (None, None) => prev_price_valid,
                // otherwise, we check the publish time
                (Some(prev), Some(curr)) => curr > prev,
            };

            if !prev_price_valid || !prev_price_usd_valid {
                set_err(perp_anyhow!(
                    ErrorId::PriceAlreadyExists,
                    ErrorDomain::Pyth,
                    "No new price data"
                ));
            }
        }

        PYTH_PREV_MARKET_PRICE.save(ctx.storage, &market_id, &market_price)?;
        LAST_BLOCK_TIME_UPDATED.save(ctx.storage, &self.now())?;

        if let Err(err) = reply_result {
            // TODO - inspect this error, bail immediately if it's not price-related?
            set_err(anyhow!("submessage error: {}", err));
        }

        // if we have an error, and we're not swallowing it, return it
        if bail_on_error {
            if let Some(err) = err {
                return Err(err);
            }
        }

        Ok(())
    }
}
