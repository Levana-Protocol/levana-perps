use msg::token::Token;

use crate::prelude::*;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Received {
    Collateral(NonZero<Collateral>),
    Lp(LpToken),
    Xlp(LpToken),
}

impl State<'_> {
    pub(crate) fn funds_received(
        &self,
        info: &MessageInfo,
        msg: &ExecuteMsg,
    ) -> Result<Option<Received>> {
        let received = match msg {
            ExecuteMsg::Receive { amount, .. } => {
                let get_lp_token = || LpToken::from_u128(amount.u128());
                let received = if info.sender == self.market_info.lp_addr {
                    Received::Lp(get_lp_token()?)
                } else if info.sender == self.market_info.xlp_addr {
                    Received::Xlp(get_lp_token()?)
                } else {
                    match &self.market_info.collateral {
                        Token::Cw20 {
                            addr,
                            decimal_places: _,
                        } => {
                            if addr.as_str() == info.sender.as_str() {
                                Received::Collateral(
                                    NonZero::<Collateral>::try_from_decimal(
                                        self.market_info.collateral.from_u128((*amount).into())?,
                                    )
                                    .context("collateral must be non-zero")?,
                                )
                            } else {
                                anyhow::bail!(
                                    "Invalid Receive called from contract {}",
                                    info.sender
                                )
                            }
                        }
                        Token::Native { .. } => anyhow::bail!(
                            "Invalid Receive for native collateral market from contract {}",
                            info.sender
                        ),
                    }
                };

                Some(received)
            }
            _ => match &self.market_info.collateral {
                Token::Native { denom, .. } => info
                    .funds
                    .iter()
                    .find(|coin| coin.denom == *denom)
                    .and_then(|coin| {
                        self.market_info
                            .collateral
                            .from_u128(coin.amount.u128())
                            .ok()
                            .and_then(NonZero::<Collateral>::try_from_decimal)
                            .map(Received::Collateral)
                    }),
                Token::Cw20 { .. } => None,
            },
        };

        // early-exit if a message doesn't require funds and the user sent (be nice)
        // or if a message does require funds and the user didn't send any
        let requires_funds = msg_requires_funds(msg);
        if requires_funds && received.is_none() {
            anyhow::bail!("{:?} requires funds", msg);
        } else if !requires_funds && received.is_some() {
            anyhow::bail!("{:?} doesn't require any funds", msg);
        }

        Ok(received)
    }
}

fn msg_requires_funds(msg: &ExecuteMsg) -> bool {
    match msg {
        ExecuteMsg::Owner(_)
        | ExecuteMsg::LockdropWithdraw { .. }
        | ExecuteMsg::Withdraw { .. }
        | ExecuteMsg::ClaimLvn { .. }
        | ExecuteMsg::Reinvest { .. }
        | ExecuteMsg::TransferBonus { .. } => false,

        ExecuteMsg::Receive { .. }
        | ExecuteMsg::LockdropDeposit { .. }
        | ExecuteMsg::Deposit { .. } => true,
    }
}
