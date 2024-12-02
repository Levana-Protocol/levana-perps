//! Error handling helpers for within the perps protocol
pub(crate) mod market;

use crate::event::CosmwasmEventExt;
use cosmwasm_std::Event;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

/// An error message for the perps protocol
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct PerpError<T = ()> {
    /// Unique identifier for this error
    pub id: ErrorId,
    /// Where in the protocol the error came from
    pub domain: ErrorDomain,
    /// User friendly description
    pub description: String,
    /// Optional additional information
    pub data: Option<T>,
}

/// Unique identifier for an error within perps
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorId {
    InvalidStakeLp,
    InvalidAmount,
    SlippageAssert,
    PriceAlreadyExists,
    PriceNotFound,
    PriceTooOld,
    PositionUpdate,
    NativeFunds,
    Cw20Funds,
    Auth,
    Expired,
    MsgValidation,
    Conversion,
    Config,
    InternalReply,
    Exceeded,
    Any,
    Stale,
    InsufficientMargin,
    InvalidLiquidityTokenMsg,
    AddressAlreadyExists,
    DeltaNeutralityFeeAlreadyLong,
    DeltaNeutralityFeeAlreadyShort,
    DeltaNeutralityFeeNewlyLong,
    DeltaNeutralityFeeNewlyShort,
    DeltaNeutralityFeeLongToShort,
    DeltaNeutralityFeeShortToLong,
    DirectionToBaseFlipped,
    MissingFunds,
    UnnecessaryFunds,
    NoYieldToClaim,
    InsufficientForReinvest,
    TimestampSubtractUnderflow,

    // Errors that come from MarketError
    InvalidInfiniteMaxGains,
    InvalidInfiniteTakeProfitPrice,
    MaxGainsTooLarge,
    WithdrawTooMuch,
    InsufficientLiquidityForWithdrawal,
    MissingPosition,
    TraderLeverageOutOfRange,
    CounterLeverageOutOfRange,
    MinimumDeposit,
    Congestion,
    MaxLiquidity,
    InvalidTriggerPrice,
    LiquidityCooldown,
    PendingDeferredExec,
    VolatilePriceFeedTimeDelta,
    LimitOrderAlreadyCanceling,
    PositionAlreadyClosing,
    NoPricePublishTimeFound,
    PositionAlreadyClosed,
    MissingTakeProfit,
    InsufficientLiquidityForUnlock,
    Liquidity,
}

/// Source within the protocol for the error
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum ErrorDomain {
    Market,
    SpotPrice,
    PositionToken,
    LiquidityToken,
    Cw20,
    Wallet,
    Factory,
    Default,
    Faucet,
    Pyth,
    Farming,
    Stride,
    SimpleOracle,
}

impl<T: Serialize> fmt::Display for PerpError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string_pretty(&self).map_err(|_| fmt::Error)?
        )
    }
}

impl<T: Serialize> fmt::Debug for PerpError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string_pretty(&self).map_err(|_| fmt::Error)?
        )
    }
}

impl PerpError {
    /// Create a new [Self] but with empty data.
    pub fn new(id: ErrorId, domain: ErrorDomain, desc: impl Into<String>) -> Self {
        PerpError {
            id,
            domain,
            description: desc.into(),
            data: None,
        }
    }

    /// Create a new auth error but with empty data.
    pub fn auth(domain: ErrorDomain, desc: impl Into<String>) -> Self {
        PerpError {
            id: ErrorId::Auth,
            domain,
            description: desc.into(),
            data: None,
        }
    }

    /// Create a new [Self] for market contract, but with no data.
    pub fn market(id: ErrorId, desc: impl Into<String>) -> Self {
        PerpError {
            id,
            domain: ErrorDomain::Market,
            description: desc.into(),
            data: None,
        }
    }

    /// Create a new [Self] for Cw20 contract with no data
    pub fn cw20(id: ErrorId, desc: impl Into<String>) -> Self {
        PerpError {
            id,
            domain: ErrorDomain::Cw20,
            description: desc.into(),
            data: None,
        }
    }
    /// Include error information into an event
    pub fn mixin_event(&self, evt: Event) -> Event {
        // these unwraps are okay, just a shorthand helper to get the enum variants as a string
        let evt = evt.add_attributes([
            ("error-id", serde_json::to_string(&self.id).unwrap()),
            ("error-domain", serde_json::to_string(&self.domain).unwrap()),
            ("error-description", self.description.to_string()),
        ]);

        match &self.data {
            None => evt,
            // this should only fail if the inner to_json_vec of serde fails. that's a (very unlikely) genuine panic situation
            Some(data) => evt.add_attribute("error-data", serde_json::to_string(data).unwrap()),
        }
    }

    /// Generate an error saying something is unimplemented
    pub fn unimplemented() -> Self {
        Self {
            id: ErrorId::Any,
            domain: ErrorDomain::Default,
            description: "unimplemented".to_string(),
            data: None,
        }
    }
}

impl TryFrom<Event> for PerpError {
    type Error = anyhow::Error;

    fn try_from(evt: Event) -> anyhow::Result<Self> {
        Ok(Self {
            id: evt.json_attr("error-id")?,
            domain: evt.json_attr("error-domain")?,
            description: evt.string_attr("error-description")?,
            data: evt.try_json_attr("error-data")?,
        })
    }
}

impl<T: Serialize> std::error::Error for PerpError<T> {}
