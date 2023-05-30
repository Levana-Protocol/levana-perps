use cosmwasm_std::from_binary;
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
        info: MessageInfo,
        msg: ExecuteMsg,
    ) -> Result<(Addr, Option<Received>, ExecuteMsg)> {
        let received = match &msg {
            ExecuteMsg::Receive { amount, .. } => {
                if !info.funds.is_empty() {
                    bail!("No native funds should be sent alongside CW20");
                }
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
        let requires_funds = msg_requires_funds(&msg);
        if requires_funds && received.is_none() {
            anyhow::bail!("{:?} requires funds", msg);
        } else if !requires_funds && received.is_some() {
            anyhow::bail!("{:?} doesn't require any funds", msg);
        }

        // replace the sender and msg with the cw20 if need-be
        let (sender, msg) = match msg {
            ExecuteMsg::Receive { sender, msg, .. } => {
                (sender.validate(self.api)?, from_binary(&msg)?)
            }
            _ => (info.sender, msg),
        };

        Ok((sender, received, msg))
    }

    pub(crate) fn get_lvn_funds(
        &self,
        info: &MessageInfo,
        store: &dyn Storage,
    ) -> Result<LvnToken> {
        let token = self.load_lvn_token(store)?;
        let denom = match &token {
            Token::Cw20 { .. } => bail!("LVN token must be Native"),
            Token::Native { denom, .. } => denom,
        };

        let funds = info.funds.iter().find(|coin| coin.denom == *denom);

        let lvn = match funds {
            None => LvnToken::zero(),
            Some(coin) => {
                let amount = token.from_u128(coin.amount.u128())?;
                LvnToken::from_decimal256(amount)
            }
        };

        Ok(lvn)
    }
}

fn msg_requires_funds(msg: &ExecuteMsg) -> bool {
    match msg {
        ExecuteMsg::Owner(_)
        | ExecuteMsg::LockdropWithdraw { .. }
        | ExecuteMsg::Withdraw { .. }
        | ExecuteMsg::ClaimEmissions { .. }
        | ExecuteMsg::ClaimLockdropRewards { .. }
        | ExecuteMsg::Reinvest { .. }
        | ExecuteMsg::TransferBonus { .. } => false,

        ExecuteMsg::Receive { .. }
        | ExecuteMsg::LockdropDeposit { .. }
        | ExecuteMsg::Deposit { .. } => true,
    }
}
