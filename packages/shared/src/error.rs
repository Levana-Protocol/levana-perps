//! Error handling helpers for within the perps protocol
use crate::{
    event::CosmwasmEventExt,
    storage::{AuthCheck, Timestamp},
};
use cosmwasm_std::{Addr, Event};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt};

/// Unique identifier for an error within perps
#[allow(missing_docs)]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum PerpError {
    #[error("failed auth, actual address: {addr}, check against: {check}")]
    Auth { addr: Addr, check: AuthCheck },
    #[error("Collateral-is-quote markets do not support infinite max gains")]
    InfiniteMaxGainsCollateralIsQuote {},
    #[error("Infinite max gains are only allowed on Long positions")]
    InfiniteMaxGainsShort {},
    #[error("Max gains too large")]
    MaxGainsTooLarge {},
    #[error("Invalid timestamp subtraction during. Action: {desc}. Values: {self} - {rhs}")]
    TimestampSubtractUnderflow {
        lhs: Timestamp,
        rhs: Timestamp,
        desc: &'static str,
    },

    #[cfg(test)]
    #[error("This is a test. Number is {number}. String is {string}.")]
    SomeTest { number: u32, string: String },
    // #[error("FIXME")]
    // InvalidWithdrawal {},
    // #[error("FIXME")]
    // InvalidStakeLp {},
    // #[error("FIXME")]
    // InvalidAmount {},
    // #[error("FIXME")]
    // SlippageAssert {},
    // #[error("FIXME")]
    // PriceAlreadyExists {},
    // #[error("FIXME")]
    // PriceNotFound {},
    // #[error("FIXME")]
    // PriceTooOld {},
    // #[error("FIXME")]
    // Liquidity {},
    // #[error("FIXME")]
    // MissingPosition {},
    // #[error("FIXME")]
    // LeverageValidation {},
    // #[error("FIXME")]
    // PositionUpdate {},
    // #[error("FIXME")]
    // NativeFunds {},
    // #[error("FIXME")]
    // Cw20Funds {},

    // #[error("FIXME")]
    // Expired {},
    // #[error("FIXME")]
    // MsgValidation {},
    // #[error("FIXME")]
    // Conversion {},
    // #[error("FIXME")]
    // Config {},
    // #[error("FIXME")]
    // InternalReply {},
    // #[error("FIXME")]
    // Exceeded {},
    // #[error("FIXME")]
    // Any {},
    // #[error("FIXME")]
    // Stale {},
    // #[error("FIXME")]
    // InsufficientMargin {},
    // #[error("FIXME")]
    // InvalidLiquidityTokenMsg {},
    // #[error("FIXME")]
    // AddressAlreadyExists {},
    // #[error("FIXME")]
    // DeltaNeutralityFeeAlreadyLong {},
    // #[error("FIXME")]
    // DeltaNeutralityFeeAlreadyShort {},
    // #[error("FIXME")]
    // DeltaNeutralityFeeNewlyLong {},
    // #[error("FIXME")]
    // DeltaNeutralityFeeNewlyShort {},
    // #[error("FIXME")]
    // DeltaNeutralityFeeLongToShort {},
    // #[error("FIXME")]
    // DeltaNeutralityFeeShortToLong {},
    // #[error("FIXME")]
    // MinimumDeposit {},
    // #[error("FIXME")]
    // DirectionToBaseFlipped {},
    // #[error("FIXME")]
    // MissingFunds {},
    // #[error("FIXME")]
    // UnnecessaryFunds {},
    // #[error("FIXME")]
    // NoYieldToClaim {},
    // #[error("FIXME")]
    // InsufficientForReinvest {},
    // #[error("FIXME")]
    // TimestampSubtractUnderflow {},
}

/// A standardized format for errors from the smart contracts.
#[derive(serde::Serialize)]
pub struct WrappedPerpError {
    id: Cow<'static, str>,
    description: String,
    data: serde_json::Value,
}

impl WrappedPerpError {
    fn from_anyhow_raw(e: anyhow::Error) -> Self {
        WrappedPerpError {
            id: "unknown".into(),
            description: e.to_string(),
            data: serde_json::Value::Null,
        }
    }

    fn from_perp_error(e: &anyhow::Error) -> Option<Self> {
        let e = e.downcast_ref::<PerpError>()?;
        let description = e.to_string();
        let value = serde_json::to_value(e.clone()).ok()?;
        let mut pairs = match value {
            serde_json::Value::Object(o) => o,
            _ => return None,
        }
        .into_iter();
        let (id, data) = pairs.next()?;
        if pairs.next().is_some() {
            return None;
        }
        Some(WrappedPerpError {
            id: id.into(),
            description,
            data,
        })
    }
}

impl From<anyhow::Error> for WrappedPerpError {
    fn from(e: anyhow::Error) -> Self {
        Self::from_perp_error(&e).unwrap_or_else(|| Self::from_anyhow_raw(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_perp_error() {
        let perp_error = PerpError::SomeTest {
            number: 42,
            string: "fortytwo".to_owned(),
        };
        let anyhow_error = anyhow::Error::from(perp_error);
        let actual = WrappedPerpError::from(anyhow_error);
        #[derive(serde::Serialize)]
        struct Data {
            number: u32,
            string: String,
        }
        let expected = WrappedPerpError {
            id: "some_test".into(),
            description: "This is a test. Number is 42. String is fortytwo.".to_owned(),
            data: serde_json::to_value(Data {
                number: 42,
                string: "fortytwo".to_owned(),
            })
            .unwrap(),
        };
        assert_eq!(
            serde_json::to_string(&actual).unwrap(),
            serde_json::to_string(&expected).unwrap()
        );
    }

    #[test]
    fn from_anyhow_error() {
        let anyhow_error = anyhow::anyhow!("Some other error");
        let actual = WrappedPerpError::from(anyhow_error);
        let expected = WrappedPerpError {
            id: "unknown".into(),
            description: "Some other error".to_owned(),
            data: serde_json::Value::Null,
        };
        assert_eq!(
            serde_json::to_string(&actual).unwrap(),
            serde_json::to_string(&expected).unwrap()
        );
    }
}
