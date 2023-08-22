//! Entrypoint messages for pyth bridge contract

use super::PythPriceFeed;
use cosmwasm_schema::QueryResponses;
use shared::prelude::*;

/// What type of feed?
#[cw_serde]
#[derive(Copy)]
pub enum FeedType {
    /// Stable CosmWasm
    ///
    /// From <https://pyth.network/developers/price-feed-ids#cosmwasm-stable>
    Stable,
    /// Edge CosmWasm
    ///
    /// From <https://pyth.network/developers/price-feed-ids#cosmwasm-edge>
    Edge,
}

impl FromStr for FeedType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "stable" => Ok(FeedType::Stable),
            "edge" => Ok(FeedType::Edge),
            _ => Err(anyhow::anyhow!(
                "Invalid feed type: {s}. Expected 'stable' or 'edge'"
            )),
        }
    }
}

/// Instantiate message for pyth bridge
#[cw_serde]
pub struct InstantiateMsg {
    /// The factory address
    pub factory: RawAddr,
    /// The Pyth address
    pub pyth: RawAddr,
    /// Does this use the stable or edge feeds?
    pub feed_type: FeedType,
    /// The number of seconds to tolerate
    pub update_age_tolerance_seconds: u32,
    /// Which market do we support?
    pub market: MarketId,
    /// feed of the base asset in terms of the quote asset
    pub feeds: Vec<PythPriceFeed>,
    /// feed of the collateral asset in terms of USD
    ///
    /// This is used by the protocol to track USD values. This field is
    /// optional, as markets with USD as the quote asset do not need to
    /// provide it.
    pub feeds_usd: Option<Vec<PythPriceFeed>>,
}

/// Same as [InstantiateMsg], but resolve [RawAddr] into [Addr].
#[cw_serde]
pub struct Config {
    /// The factory address
    pub factory: Addr,
    /// The Pyth address
    pub pyth: Addr,
    /// Does this use the stable or edge feeds?
    pub feed_type: FeedType,
    /// The number of seconds to tolerate
    pub update_age_tolerance_seconds: u32,
    /// Which market do we support?
    pub market: MarketId,
    /// feed of the base asset in terms of the quote asset
    pub feeds: Vec<PythPriceFeed>,
    /// feed of the collateral asset in terms of USD
    ///
    /// This is used by the protocol to track USD values. This field is
    /// optional, as markets with USD as the quote asset do not need to
    /// provide it.
    pub feeds_usd: Option<Vec<PythPriceFeed>>,
}

/// Execute message for pyth bridge
#[cw_serde]
pub enum ExecuteMsg {
    /// Updates the price
    /// This is not permissioned, anybody can call it
    UpdatePrice {
        /// How many executions of the crank to perform
        ///
        /// Each time a price is updated in the system, cranking is immediately
        /// necessary to check for liquidations. As an optimization, the
        /// protocol includes that cranking as part of price updating. The value
        /// here represents how many turns of the crank should be performed, or
        /// use [None] for the default.
        execs: Option<u32>,
        /// Which wallet receives crank rewards.
        ///
        /// If unspecified, this defaults to the caller
        /// (of the bridge, not the bridge itself)
        rewards: Option<RawAddr>,

        /// If true: then an error (such as no new price) bails out with Err
        /// if false (the default): sets the error string on the response's data field and returns Ok
        #[serde(default)]
        bail_on_error: bool,
    },
}

/// Query message for liquidity token proxy
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [Config]
    #[returns(Config)]
    Config {},
    /// * returns [super::MarketPrice]
    ///
    /// The prices for a given market
    #[returns(super::MarketPrice)]
    MarketPrice {
        /// How long can the price be stale for
        age_tolerance_seconds: u32,
    },
    /// * returns [cw2::ContractVersion]
    #[returns(cw2::ContractVersion)]
    Version {},
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}
