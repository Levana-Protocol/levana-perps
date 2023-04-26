//! Represents the native coin or CW20 used for collateral in a market.
use crate::contracts::{
    cw20::entry::{
        BalanceResponse as Cw20BalanceResponse, ExecuteMsg as Cw20ExecuteMsg,
        QueryMsg as Cw20QueryMsg,
    },
    market::entry::ExecuteMsg as MarketExecuteMsg,
};
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal256, QuerierWrapper, WasmMsg,
};
use shared::prelude::*;

use serde::Serialize;

/// The number of decimal places for tokens may vary
/// and there is a smart query cost for deriving it at runtime
/// so we grab the info at init time and then store it as a full-fledged token
#[cw_serde]
pub enum TokenInit {
    /// A cw20 address. Decimal places will be derived.
    Cw20 {
        /// Address of the CW20 contract
        addr: RawAddr,
    },

    /// Native currency. May cover some IBC tokens too
    Native {
        /// Denom used within the chain for this native coin
        denom: String,
    },
}

impl From<Token> for TokenInit {
    fn from(src: Token) -> Self {
        match src {
            Token::Native { denom, .. } => Self::Native { denom },
            Token::Cw20 { addr, .. } => Self::Cw20 { addr },
        }
    }
}

/// The overall ideas of the Token API are:
/// 1. use the Number type, not u128 or Uint128
/// 2. abstract over the Cw20/Native variants
///
/// At the end of the day, call transfer/query with
/// the same business logic as contract math
/// and don't worry at all about conversions or addresses/denoms
#[cw_serde]
pub enum Token {
    /// An asset controlled by a CW20 token.
    Cw20 {
        /// Address of the contract
        addr: RawAddr,
        /// Decimals places used by the contract
        decimal_places: u8,
    },

    /// Native coin on the blockchain
    Native {
        /// Native coin denom string
        denom: String,
        /// Decimal places used by the asset
        decimal_places: u8,
    },
}

impl Token {
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Native { denom, .. } => {
                format!("native-{}", denom)
            }
            Self::Cw20 { addr, .. } => {
                format!("cw20-{}", addr)
            }
        }
    }

    pub(crate) fn decimal_places(&self) -> u8 {
        match self {
            Self::Native { decimal_places, .. } => *decimal_places,
            Self::Cw20 { decimal_places, .. } => *decimal_places,
        }
    }
    /// This is the usual function to call for transferring money
    /// the result can simply be added as a Message to any Response
    /// the amount is expressed as Number such that it mirrors self.query_balance()
    pub fn into_transfer_msg(
        &self,
        recipient: &Addr,
        amount: NonZero<Collateral>,
    ) -> Result<Option<CosmosMsg>> {
        match self {
            Self::Native { .. } => {
                let coin = self.into_native_coin(amount.raw())?;

                match coin {
                    Some(coin) => Ok(Some(CosmosMsg::Bank(BankMsg::Send {
                        to_address: recipient.to_string(),
                        amount: vec![coin],
                    }))),

                    None => Ok(None),
                }
            }
            Self::Cw20 { addr, .. } => {
                let msg = self.into_cw20_execute_transfer_msg(recipient, amount)?;

                match msg {
                    Some(msg) => {
                        let msg = to_binary(&msg)?;

                        Ok(Some(CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: addr.to_string(),
                            msg,
                            funds: Vec::new(),
                        })))
                    }
                    None => Ok(None),
                }
            }
        }
    }

    /// Get the balance - this is expressed as Number
    /// such that it mirrors self.into_transfer_msg()
    pub fn query_balance(&self, querier: &QuerierWrapper, user_addr: &Addr) -> Result<Collateral> {
        self.from_u128(match self {
            Self::Cw20 { addr, .. } => {
                let resp: Cw20BalanceResponse = querier.query_wasm_smart(
                    addr.as_str(),
                    &Cw20QueryMsg::Balance {
                        address: user_addr.to_string().into(),
                    },
                )?;

                resp.balance.u128()
            }
            Self::Native { denom, .. } => {
                let coin = querier.query_balance(user_addr, denom)?;
                coin.amount.u128()
            }
        })
        .map(Collateral::from_decimal256)
    }
    /// helper function
    ///
    /// given a u128, typically via a native Coin.amount or Cw20 amount
    /// get the Decimal256 representation according to the WalletSource's config
    ///
    /// this is essentially the inverse of self.into_u128()
    pub fn from_u128(&self, amount: u128) -> Result<Decimal256> {
        Decimal256::from_atomics(amount, self.decimal_places().into()).map_err(|e| e.into())
    }

    /// helper function
    ///
    /// given a number, typically via business logic and client API
    /// get the u128 representation, e.g. for Coin or Cw20
    /// according to the WalletSource's config
    ///
    /// this will only return None if the amount is zero (or rounds to 0)
    /// which then bubbles up into other methods that build on this
    ///
    /// this is essentially the inverse of self.from_u128()
    pub fn into_u128(&self, amount: Decimal256) -> Result<Option<u128>> {
        let value: u128 = amount
            .into_number()
            .to_u128_with_precision(self.decimal_places().into())
            .ok_or_else(|| {
                perp_anyhow!(
                    ErrorId::Conversion,
                    ErrorDomain::Wallet,
                    "{} unable to convert {} to u128!",
                    self.name(),
                    amount
                )
            })?;

        if value > 0 {
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// helper function
    ///
    /// when we know for a fact we have a WalletSource::native
    /// we can get a Coin from a Number amount
    pub fn into_native_coin(&self, amount: Collateral) -> Result<Option<Coin>> {
        match self {
            Self::Native { denom, .. } => {
                Ok(self
                    .into_u128(amount.into_decimal256())?
                    .map(|amount| Coin {
                        denom: denom.clone(),
                        amount: amount.into(),
                    }))
            }
            Self::Cw20 { .. } => Err(perp_anyhow!(
                ErrorId::NativeFunds,
                ErrorDomain::Wallet,
                "{} cannot be turned into a native coin",
                self.name()
            )),
        }
    }

    /// helper function
    ///
    /// when we know for a fact we have a WalletSource::Cw20
    /// we can get a Send Execute messge from a Number amount
    pub fn into_cw20_execute_send_msg<T: Serialize>(
        &self,
        contract: &Addr,
        amount: Collateral,
        submsg: &T,
    ) -> Result<Option<Cw20ExecuteMsg>> {
        match self {
            Self::Native { .. } => Err(perp_anyhow!(
                ErrorId::Cw20Funds,
                ErrorDomain::Wallet,
                "{} cannot be turned into a cw20 message",
                self.name()
            )),
            Self::Cw20 { .. } => {
                let msg = to_binary(submsg)?;
                Ok(self
                    .into_u128(amount.into_decimal256())?
                    .map(|amount| Cw20ExecuteMsg::Send {
                        contract: contract.into(),
                        amount: amount.into(),
                        msg,
                    }))
            }
        }
    }

    /// helper function
    ///
    /// when we know for a fact we have a WalletSource::Cw20
    /// we can get a Transfer Execute messge from a Number amount
    pub fn into_cw20_execute_transfer_msg(
        &self,
        recipient: &Addr,
        amount: NonZero<Collateral>,
    ) -> Result<Option<Cw20ExecuteMsg>> {
        match self {
            Self::Native { .. } => Err(perp_anyhow!(
                ErrorId::Cw20Funds,
                ErrorDomain::Wallet,
                "{} cannot be turned into a cw20 message",
                self.name()
            )),
            Self::Cw20 { .. } => Ok(self.into_u128(amount.into_decimal256())?.map(|amount| {
                Cw20ExecuteMsg::Transfer {
                    recipient: recipient.into(),
                    amount: amount.into(),
                }
            })),
        }
    }

    /// perps-specific use-case for executing a market message with funds
    pub fn into_market_execute_msg(
        &self,
        market_addr: &Addr,
        amount: Collateral,
        execute_msg: MarketExecuteMsg,
    ) -> Result<WasmMsg> {
        match self.clone() {
            Self::Cw20 { addr, .. } => {
                let msg = self
                    .into_cw20_execute_send_msg(market_addr, amount, &execute_msg)
                    .map_err(|err| {
                        perp_anyhow!(
                            ErrorId::Conversion,
                            ErrorDomain::Wallet,
                            "{} (market exec inner msg: {:?})!",
                            err.downcast_ref::<PerpError>().unwrap().description,
                            execute_msg
                        )
                    })?;

                match msg {
                    Some(msg) => Ok(WasmMsg::Execute {
                        contract_addr: addr.into_string(),
                        msg: to_binary(&msg)?,
                        funds: Vec::new(),
                    }),
                    None => {
                        // no funds, so just send the execute_msg directly
                        // to the contract
                        Ok(WasmMsg::Execute {
                            contract_addr: market_addr.to_string(),
                            msg: to_binary(&execute_msg)?,
                            funds: Vec::new(),
                        })
                    }
                }
            }
            Self::Native { .. } => {
                let coin = self.into_native_coin(amount).map_err(|err| {
                    perp_anyhow!(
                        ErrorId::Conversion,
                        ErrorDomain::Wallet,
                        "{} (market exec inner msg: {:?})!",
                        err.downcast_ref::<PerpError>().unwrap().description,
                        execute_msg
                    )
                })?;

                let execute_msg = to_binary(&execute_msg)?;

                let funds = match coin {
                    Some(coin) => {
                        vec![coin]
                    }
                    None => Vec::new(),
                };

                Ok(WasmMsg::Execute {
                    contract_addr: market_addr.to_string(),
                    msg: execute_msg,
                    funds,
                })
            }
        }
    }

    /// Validates that the given collateral doesn't require more precision
    /// than what the token supports
    pub fn validate_collateral(&self, value: NonZero<Collateral>) -> Result<NonZero<Collateral>> {
        let value_decimal256 = value.into_decimal256();

        if let Some(value_128) = self.into_u128(value_decimal256)? {
            let value_truncated = self.from_u128(value_128)?;
            if value_truncated == value_decimal256 {
                return Ok(value);
            }
        }

        Err(perp_anyhow!(
            ErrorId::Conversion,
            ErrorDomain::Wallet,
            "Token Collateral must be as precise as the Token (is {}, only {} decimal places supported)", value, self.decimal_places()
        ))
    }
}
