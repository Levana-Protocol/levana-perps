//! Messages for the pyth bridge contract.
//!
//! This contract is used to abstract the Pyth oracle from the rest of the
//! protocol. It is responsible for updating the price of a market.

use cosmwasm_schema::cw_serde;
use pyth_sdk_cw::{PriceIdentifier, UnixTimestamp};
use shared::storage::{PriceBaseInQuote, PriceCollateralInUsd};

pub mod entry;
pub mod events;

/// Price feed
#[cw_serde]
pub struct PythPriceFeed {
    /// The price feed id
    pub id: PriceIdentifier,
    /// is this price feed inverted
    pub inverted: bool,
}

/// Prices for a given market
#[cw_serde]
pub struct MarketPrice {
    /// Price of the base asset in terms of the quote asset
    pub price: PriceBaseInQuote,
    /// Price of the collateral asset in terms of USD
    ///
    /// This is used by the protocol to track USD values. This field is
    /// optional, as markets with USD as the quote asset do not need to
    /// provide it.
    pub price_usd: Option<PriceCollateralInUsd>,

    /// Latest price publish time for the feeds composing the price
    pub latest_price_publish_time: UnixTimestamp,
    /// Latest price publish time for the feeds composing the price_usd
    pub latest_price_usd_publish_time: Option<UnixTimestamp>,
}
