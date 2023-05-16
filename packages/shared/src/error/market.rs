//! Special error types for the market contract.
//!
//! This module is intended as a stop-gap measure. The perps protocol overall
//! uses `anyhow` for error handling, and then uses a `PerpError` type to
//! represent known error cases that require special handling by consumers of
//! the contracts.
//!
//! Generally we would like to move `PerpError` over to using `thiserror`, and
//! then have a duality of error handling: `anyhow::Error` for general purpose
//! errors (like serialization issues) that do not require special handling, and
//! `PerpError` for well described error types with known payloads. However,
//! making such a change would be an invasive change to the codebase.
//!
//! Instead, in the short term, we use this module to provide well-typed
//! `thiserror` error values that can be converted to `PerpError` values.

use crate::prelude::*;

/// An error type for known market errors with potentially special error handling.
#[derive(thiserror::Error, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "snake_case", tag = "error_id")]
#[allow(missing_docs)]
pub enum MarketError {
    #[error(
        "Infinite max gains can only be used on long positions for collateral-is-base markets"
    )]
    InvalidInfiniteMaxGains {
        market_type: MarketType,
        direction: DirectionToBase,
    },
    #[error("Max gains are too large")]
    MaxGainsTooLarge {},
    #[error("Unable to withdraw {requested}. Only {available} LP tokens held.")]
    WithdrawTooMuch {
        requested: NonZero<LpToken>,
        available: NonZero<LpToken>,
    },
    #[error("Insufficient unlocked liquidity for withdrawal. Requested {requested_collateral} ({requested_lp} LP tokens), only {unlocked} liquidity available.")]
    InsufficientLiquidityForWithdrawal {
        requested_lp: NonZero<LpToken>,
        requested_collateral: NonZero<Collateral>,
        unlocked: Collateral,
    },
    #[error("Missing position: {id}")]
    MissingPosition { id: String },
    #[error("Trader leverage {new_leverage} is out of range ({low_allowed}..{high_allowed}]")]
    TraderLeverageOutOfRange {
        low_allowed: Decimal256,
        high_allowed: Decimal256,
        new_leverage: Decimal256,
        current_leverage: Option<Decimal256>,
    },
    #[error("Counter leverage {new_leverage} is out of range ({low_allowed}..{high_allowed}]")]
    CounterLeverageOutOfRange {
        low_allowed: Decimal256,
        high_allowed: Decimal256,
        new_leverage: Decimal256,
        current_leverage: Option<Decimal256>,
    },
    #[error("Deposit collateral is too small. Deposited {deposit_collateral}, or {deposit_usd} USD. Minimum is {minimum_usd} USD")]
    MinimumDeposit {
        deposit_collateral: Collateral,
        deposit_usd: Usd,
        minimum_usd: Usd,
    },
}

impl MarketError {
    /// Convert into an `anyhow::Error`.
    ///
    /// This method will first convert into a `PerpError` and then wrap that
    /// in `anyhow::Error`.
    pub fn into_anyhow(self) -> anyhow::Error {
        let description = format!("{self}");
        self.into_perp_error(description).into()
    }

    /// Try to convert from an `anyhow::Error`.
    pub fn try_from_anyhow(err: &anyhow::Error) -> Result<Self> {
        (|| {
            let err = err
                .downcast_ref::<PerpError<MarketError>>()
                .context("Not a PerpError<MarketError>")?;
            err.data
                .clone()
                .context("PerpError<MarketError> without a data field")
        })()
        .with_context(|| format!("try_from_anyhow failed on: {err:?}"))
    }

    /// Convert into a `PerpError`.
    fn into_perp_error(self, description: String) -> PerpError<MarketError> {
        let id = self.get_error_id();
        PerpError {
            id,
            domain: ErrorDomain::Market,
            description,
            data: Some(self),
        }
    }

    /// Get the [ErrorId] for this value.
    fn get_error_id(&self) -> ErrorId {
        match self {
            MarketError::InvalidInfiniteMaxGains { .. } => ErrorId::InvalidInfiniteMaxGains,
            MarketError::MaxGainsTooLarge {} => ErrorId::MaxGainsTooLarge,
            MarketError::WithdrawTooMuch { .. } => ErrorId::WithdrawTooMuch,
            MarketError::InsufficientLiquidityForWithdrawal { .. } => {
                ErrorId::InsufficientLiquidityForWithdrawal
            }
            MarketError::MissingPosition { .. } => ErrorId::MissingPosition,
            MarketError::TraderLeverageOutOfRange { .. } => ErrorId::TraderLeverageOutOfRange,
            MarketError::CounterLeverageOutOfRange { .. } => ErrorId::CounterLeverageOutOfRange,
            MarketError::MinimumDeposit { .. } => ErrorId::MinimumDeposit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_perp_error() {
        let market_error = MarketError::WithdrawTooMuch {
            requested: "100".parse().unwrap(),
            available: "50".parse().unwrap(),
        };
        let expected = PerpError {
            id: ErrorId::WithdrawTooMuch,
            domain: ErrorDomain::Market,
            description: "Unable to withdraw 100. Only 50 LP tokens held.".to_owned(),
            data: Some(market_error.clone()),
        };
        let anyhow_error = market_error.clone().into_anyhow();
        let actual = anyhow_error.downcast_ref::<PerpError<_>>().unwrap();
        assert_eq!(&expected, actual);

        let market_error2 = MarketError::try_from_anyhow(&anyhow_error).unwrap();
        assert_eq!(market_error, market_error2);
    }
}
