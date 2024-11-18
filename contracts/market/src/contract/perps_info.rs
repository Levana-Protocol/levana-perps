//! Helper module for ensuring correct collateral is sent into the contract.
//!
//! We can receive incoming collateral as either native funds or a CW20 Receive message. This module provides assistance to ensure that:
//!
//! * Only the correct type of collateral is sent to this contract
//!
//! * Any collateral that is sent is actually gets used

use anyhow::ensure;
use cosmwasm_std::{from_json, MessageInfo};
use perpswap::token::Token;

use crate::prelude::*;

/// Perps-specific message info handling native coins versus CW20.
///
/// This data type represents the true sender and message, regardless of whether
/// funds were sent with a CW20 or native coins. It also handles parsing and
/// validation of submitted collateral amounts.
pub(crate) struct PerpsMessageInfo {
    /// The amount of collateral sent in.
    pub(crate) funds: CollateralSent,
    /// The true message, potentially parsed from a CW20 receive.
    pub(crate) msg: MarketExecuteMsg,
    /// The true sender, potentially parsed from a CW20 receive.
    pub(crate) sender: Addr,
}

/// Collateral sent into the contract with a message.
///
/// Note that the amount sent in is kept as a private field here intentionally.
/// Callers should use the [CollateralSent::take] method if they need the
/// amount. Callers should also ensure that they always call
/// [CollateralSent::ensure_empty] after using any collateral.
pub(crate) struct CollateralSent {
    /// The amount sent in.
    amount: Option<NonZero<Collateral>>,
}

impl State<'_> {
    /// Parse the message to handle CW20 receive and determine any collateral sent in.
    pub(crate) fn parse_perps_message_info(
        &self,
        store: &dyn Storage,
        info: MessageInfo,
        msg: MarketExecuteMsg,
    ) -> Result<PerpsMessageInfo> {
        match msg {
            ExecuteMsg::Receive {
                sender,
                amount,
                msg,
            } => {
                // CW20 receive message, parse the inner information and ensure this was the correct contract.
                let msg: ExecuteMsg = from_json(msg)?;

                let source = self.get_token(store)?;
                let funds = match source {
                    Token::Native { .. } => {
                        return Err(perp_anyhow!(
                            ErrorId::Cw20Funds,
                            ErrorDomain::Market,
                            "native assets come through execute messages directly"
                        ));
                    }
                    Token::Cw20 {
                        addr,
                        decimal_places,
                    } => {
                        if addr.as_str() != info.sender.as_str() {
                            return Err(perp_anyhow!(
                                ErrorId::Cw20Funds,
                                ErrorDomain::Market,
                                "Wrong CW20 address. Expected: {addr}. Receive: {sender}."
                            ));
                        }
                        NonZero::new(Collateral::from_decimal256(Decimal256::from_atomics(
                            amount.u128(),
                            (*decimal_places).into(),
                        )?))
                        .context("Cannot send 0 tokens into the contract")?
                    }
                };

                ensure!(
                    info.funds.is_empty(),
                    PerpError::market(ErrorId::Cw20Funds, "Sent native funds to a CW20 market")
                );

                Ok(PerpsMessageInfo {
                    funds: CollateralSent {
                        amount: Some(funds),
                    },
                    msg,
                    sender: sender.validate(self.api)?,
                })
            }
            msg => {
                // Not a CW20 receive. First thing we do is check if any native
                // coins were sent in.
                let mut funds = info.funds.into_iter();
                let coin = match funds.next() {
                    // Found some native coins, we'll deal with it below.
                    Some(coin) => coin,
                    None => {
                        // No native coins sent in, so no more parsing
                        // necessary. Both the CW20 and native code paths
                        // converge here.
                        return Ok(PerpsMessageInfo {
                            funds: CollateralSent { amount: None },
                            msg,
                            sender: info.sender,
                        });
                    }
                };
                // We got one coin already. Make sure there are no more. If
                // there are more, the caller sent in too many kinds of coins
                // and we should exit.
                ensure!(
                    funds.next().is_none(),
                    PerpError::market(ErrorId::NativeFunds, "More than 1 denom of coins attached")
                );

                match self.get_token(store)? {
                    Token::Native {
                        denom,
                        decimal_places,
                    } => {
                        // This contract expects a native coin, make sure the
                        // user sent the right kind.
                        ensure!(
                            coin.denom == *denom,
                            PerpError::market(
                                ErrorId::NativeFunds,
                                format!(
                                    "Expected native coin denom {denom}, received {}",
                                    coin.denom
                                )
                            )
                        );

                        // Convert from the native coin representation to a
                        // Collateral value.
                        let n = Decimal256::from_atomics(coin.amount, (*decimal_places).into())?;
                        let n = Collateral::from_decimal256(n);
                        let amount = match NonZero::new(n) {
                            Some(n) => Ok(n),
                            None => Err(perp_anyhow!(
                                ErrorId::NativeFunds,
                                ErrorDomain::Market,
                                "no coin amount!"
                            )),
                        }?;
                        Ok(PerpsMessageInfo {
                            funds: CollateralSent {
                                amount: Some(amount),
                            },
                            msg,
                            sender: info.sender,
                        })
                    }
                    // We received native funds, but this contract is expecting
                    // a CW20.
                    Token::Cw20 { .. } => Err(perp_anyhow!(
                        ErrorId::NativeFunds,
                        ErrorDomain::Market,
                        "direct deposit cannot be done via cw20"
                    )),
                }
            }
        }
    }
}

impl CollateralSent {
    /// Take the collateral amount, if present. Can only be called once.
    pub(crate) fn take(&mut self) -> Result<NonZero<Collateral>> {
        self.amount.take().ok_or_else(|| {
            perp_anyhow!(
                ErrorId::MissingFunds,
                ErrorDomain::Market,
                "No funds sent for message that requires funds"
            )
        })
    }

    pub(crate) fn ensure_empty(mut self) -> Result<()> {
        match self.amount.take() {
            None => Ok(()),
            Some(amount) => Err(perp_anyhow!(
                ErrorId::UnnecessaryFunds,
                ErrorDomain::Market,
                "Funds sent for message that requires none. Amount: {amount}"
            )),
        }
    }
}
