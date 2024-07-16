use crate::prelude::*;

#[must_use]
enum Funds {
    NoFunds,
    Funds { token: Token, amount: Uint128 },
}

impl Funds {
    fn require_none(self) -> Result<()> {
        match self {
            Funds::NoFunds => Ok(()),
            Funds::Funds { token, amount } => Err(Error::UnnecessaryFunds { token, amount }),
        }
    }

    fn require_some(self, market: MarketId) -> Result<Collateral> {
        match self {
            Funds::NoFunds => Err(Error::MissingRequiredFunds),
            Funds::Funds { token, amount } => {
                todo!()
            }
        }
    }
}

struct HandleFunds {
    funds: Funds,
    msg: ExecuteMsg,
    sender: Addr,
}

fn handle_funds(api: &dyn Api, mut info: MessageInfo, msg: ExecuteMsg) -> Result<HandleFunds> {
    match msg {
        ExecuteMsg::Receive {
            sender,
            amount,
            msg,
        } => {
            if info.funds.is_empty() {
                let msg: ExecuteMsg = from_json(msg).context("Invalid msg in CW20 receive")?;
                Ok(HandleFunds {
                    funds: Funds::Funds {
                        token: Token::Cw20(info.sender),
                        amount,
                    },
                    msg,
                    sender: sender
                        .validate_raw(api)
                        .context("Unable to parse CW20 receive message's sender field")?,
                })
            } else {
                Err(Error::FundsWithCw20)
            }
        }
        msg => {
            let funds = match info.funds.pop() {
                None => Funds::NoFunds,
                Some(Coin { denom, amount }) => {
                    if !info.funds.is_empty() {
                        return Err(Error::MultipleNativeFunds);
                    }
                    Funds::Funds {
                        token: Token::Native(denom),
                        amount,
                    }
                }
            };
            Ok(HandleFunds {
                funds,
                msg,
                sender: info.sender,
            })
        }
    }
}

#[entry_point]
pub fn execute(deps: DepsMut, _env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let HandleFunds { funds, msg, sender } = handle_funds(deps.api, info, msg)?;
    match msg {
        ExecuteMsg::Receive { .. } => Err(Error::ReceiveInsideReceive),
        ExecuteMsg::Deposit { market } => {
            // let funds = funds.require_some()?;
            todo!()
            // deposit(deps, sender, funds, market)
            // let old_shares = crate::state::SHARES
            //     .may_load(deps.storage, (&info.sender, &market))
            //     .context("Could not load old shares")?
            //     .map(NonZero::raw)
            //     .unwrap_or_default();
            // let old_total = crate::state::TOTALS
            //     .may_load(deps.storage, &market)
            //     .context("Could not load old total shares")?
            //     .map(NonZero::raw)
            //     .unwrap_or_default();
            // todo!()
            // // let old_collateral = COLLATERAL
        }
        ExecuteMsg::Withdraw { amount } => todo!(),
        ExecuteMsg::Balance { market } => todo!(),
        ExecuteMsg::AppointAdmin { admin } => todo!(),
        ExecuteMsg::AcceptAdmin {} => todo!(),
        ExecuteMsg::UpdateConfig(_) => todo!(),
    }
}

fn deposit(deps: DepsMut, sender: Addr, funds: Collateral, market: MarketId) -> Result<Response> {
    todo!()
}
