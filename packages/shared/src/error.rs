//! Error handling helpers for within the perps protocol
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
    InvalidWithdrawal,
    InvalidStakeLp,
    InvalidAmount,
    SlippageAssert,
    PriceAlreadyExists,
    PriceNotFound,
    PriceTooOld,
    Liquidity,
    MissingPosition,
    LeverageValidation,
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
    MinimumDeposit,
    DirectionToBaseFlipped,
    MissingFunds,
    UnnecessaryFunds,
    NoYieldToClaim,
    InsufficientForReinvest,
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
}

/// Generate a [PerpError] value
#[macro_export]
macro_rules! perp_error {
    ($id:expr, $domain:expr, $($t:tt)*) => {{
        $crate::error::PerpError {
            id: $id,
            domain: $domain,
            description: format!($($t)*),
            data: None as Option<()>,
        }
    }};
}

/// Generate a [PerpError] value with additional optional data
#[macro_export]
macro_rules! perp_error_data {
    ($id:expr, $domain:expr, $data:expr, $($t:tt)*) => {{
        $crate::error::PerpError {
            id: $id,
            domain: $domain,
            description: format!($($t)*),
            data: Some($data),
        }
    }};
}

/// Generate a [PerpError] and then wrap it up in an anyhow error
#[macro_export]
macro_rules! perp_anyhow {
    ($id:expr, $domain:expr, $($t:tt)*) => {{
        anyhow::Error::new($crate::error::PerpError {
            id: $id,
            domain: $domain,
            description: format!($($t)*),
            data: None as Option<()>,
        })
    }};
}

/// Like [perp_anyhow] but accepts optional extra data
#[macro_export]
macro_rules! perp_anyhow_data {
    ($id:expr, $domain:expr, $data:expr, $($t:tt)*) => {{
        anyhow::Error::new($crate::error::PerpError {
            id: $id,
            domain: $domain,
            description: format!($($t)*),
            data: Some($data),
        })
    }};
}

/// Ensure a condition is true, otherwise returns from the function with an error.
#[macro_export]
macro_rules! perp_ensure {
    ($val:expr, $id:expr, $domain:expr, $($t:tt)*) => {{
        if !$val {
            return Err(anyhow::Error::new($crate::error::PerpError {
                id: $id,
                domain: $domain,
                description: format!($($t)*),
                data: None as Option<()>,
            }));
        }
    }};
}

/// Return early with the given perp error
#[macro_export]
macro_rules! perp_bail {
    ($id:expr, $domain:expr, $($t:tt)*) => {{
        return Err(anyhow::Error::new($crate::error::PerpError {
            id: $id,
            domain: $domain,
            description: format!($($t)*),
            data: None as Option<()>,
        }));
    }};
}

/// Like [perp_bail] but takes extra optional data
#[macro_export]
macro_rules! perp_bail_data {
    ($id:expr, $domain:expr, $data:expr,  $($t:tt)*) => {{
        return Err(anyhow::Error::new($crate::error::PerpError {
            id: $id,
            domain: $domain,
            description: format!($($t)*),
            data: Some($data),
        }));
    }};
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
            // this should only fail if the inner to_vec of serde fails. that's a (very unlikely) genuine panic situation
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
