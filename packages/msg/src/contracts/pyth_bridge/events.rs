//! Events for pyth bridge contract
use cosmwasm_std::Event;
use shared::prelude::*;

/// Triggered whenever a new price feed is set for a market
pub struct UpdatePriceEvent {
    /// The market to update the price for
    pub market_id: MarketId,
    /// Price of the base asset in terms of the quote asset
    pub price: PriceBaseInQuote,
    /// Price of the collateral asset in terms of USD
    ///
    /// This is used by the protocol to track USD values. This field is
    /// optional, as markets with USD as the quote asset do not need to
    /// provide it.
    pub price_usd: Option<PriceCollateralInUsd>,
}

impl PerpEvent for UpdatePriceEvent {}
impl From<UpdatePriceEvent> for Event {
    fn from(src: UpdatePriceEvent) -> Self {
        let mut attrs = vec![
            ("market-id", src.market_id.to_string()),
            ("price", src.price.to_string()),
        ];

        if let Some(price_usd) = src.price_usd {
            attrs.push(("price-usd", price_usd.to_string()));
        }

        Event::new("update-price").add_attributes(attrs)
    }
}
