//! Messages for the perps liquidity token contract.
//!
//! The liquidity token is a proxy providing a CW20 interface for the LP and xLP
//! balances within a single market.
pub mod entry;

use cosmwasm_schema::cw_serde;

/// The kind of liquidity token
#[cw_serde]
#[derive(Copy)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum LiquidityTokenKind {
    /// LP token
    Lp,
    /// xLP token
    Xlp,
}
