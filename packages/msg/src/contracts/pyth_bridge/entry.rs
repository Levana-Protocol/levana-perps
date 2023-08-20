//! Entrypoint messages for pyth bridge contract

use super::PythMarketPriceFeeds;
use cosmwasm_schema::QueryResponses;
use shared::prelude::*;

/// Instantiate message for pyth bridge
#[cw_serde]
pub struct InstantiateMsg {
    /// The factory address
    pub factory: RawAddr,
    /// The Pyth address
    pub pyth: RawAddr,
    /// The number of seconds to tolerate
    pub update_age_tolerance_seconds: u32,
    /// Initial price feeds
    pub feeds: Vec<MarketFeeds>,
}

/// Market feeds to set when initiating the contract
#[cw_serde]
pub struct MarketFeeds {
    /// The market to set the price for
    pub market_id: MarketId,
    /// The Pyth price feeds
    pub market_price_feeds: PythMarketPriceFeeds,
}

/// Execute message for pyth bridge
#[cw_serde]
pub enum ExecuteMsg {
    /// Sets the price feeds for a market
    /// This is requires admin authentication
    SetMarketPriceFeeds {
        /// The market to set the price for
        market_id: MarketId,
        /// The Pyth price feeds
        market_price_feeds: PythMarketPriceFeeds,
    },
    /// Sets the age tolerance for price updates
    /// This requires admin authentication
    SetUpdateAgeTolerance {
        /// The number of seconds to tolerate
        seconds: u32,
    },
    /// Sets the Pyth price oracle contract to use
    SetPythOracle {
        /// The Pyth oracle address
        pyth: RawAddr,
    },
    /// Updates the price
    /// This is not permissioned, anybody can call it
    UpdatePrice {
        /// The market to update the price for
        market_id: MarketId,
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

impl ExecuteMsg {
    /// Returns true if the message requires admin authentication
    pub fn requires_admin(&self) -> bool {
        match self {
            ExecuteMsg::SetMarketPriceFeeds { .. } | ExecuteMsg::SetUpdateAgeTolerance { .. } => {
                true
            }
            ExecuteMsg::SetPythOracle { .. } => true,
            ExecuteMsg::UpdatePrice { .. } => false,
        }
    }
}

/// Query message for liquidity token proxy
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [cosmwasm_std::Addr]
    ///
    /// The pyth address
    #[returns(cosmwasm_std::Addr)]
    PythAddress {},
    /// * returns [super::AllPythMarketPriceFeeds]
    ///
    /// The price feeds for all markets
    #[returns(super::AllPythMarketPriceFeeds)]
    AllMarketPriceFeeds {
        /// Last market id seen (for pagination)
        start_after: Option<MarketId>,
        /// Number of markets to return
        limit: Option<u32>,
        /// Whether to return ascending or descending
        order: Option<OrderInMessage>,
    },
    /// * returns [PythMarketPriceFeeds]
    ///
    /// The price feeds for a given market
    #[returns(PythMarketPriceFeeds)]
    MarketPriceFeeds {
        /// The market to get the price feeds for
        market_id: MarketId,
    },
    /// * returns [super::MarketPrice]
    ///
    /// The prices for a given market
    #[returns(super::MarketPrice)]
    MarketPrice {
        /// The market to get the prices for
        market_id: MarketId,
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
