use anyhow::{anyhow, ensure, Context, Result};

use crate::{prelude::*, types::State};

#[must_use]
enum Funds {
    NoFunds,
    Funds { token: Token, amount: Uint128 },
}

impl Funds {
    fn require_none(self) -> Result<()> {
        match self {
            Funds::NoFunds => Ok(()),
            Funds::Funds { token, amount } => {
                Err(anyhow!("Unnecessary funds sent: {amount}{token:?}"))
            }
        }
    }

    fn require_some(self, token: &msg::token::Token) -> Result<NonZero<Collateral>> {
        match self {
            Funds::NoFunds => Err(anyhow!(
                "Message requires attached funds, but none were provided"
            )),
            Funds::Funds {
                token: fund_token,
                amount,
            } => {
                fund_token.ensure_matches(token)?;
                let collateral = token
                    .from_u128(amount.u128())
                    .context("Error converting token amount to Collateral")?;
                NonZero::new(Collateral::from_decimal256(collateral))
                    .context("Impossible 0 collateral provided")
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
                        .validate(api)
                        .context("Unable to parse CW20 receive message's sender field")?,
                })
            } else {
                Err(anyhow!(
                    "Cannot attach funds when performing a CW20 receive"
                ))
            }
        }
        msg => {
            let funds = match info.funds.pop() {
                None => Funds::NoFunds,
                Some(Coin { denom, amount }) => {
                    ensure!(
                        info.funds.is_empty(),
                        "Multiple funds provided, messages only support one fund denom"
                    );
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
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let HandleFunds { funds, msg, sender } = handle_funds(deps.api, info, msg)?;
    let (state, storage) = State::load_mut(deps, env)?;
    match msg {
        ExecuteMsg::Receive { .. } => Err(anyhow!("Cannot perform a receive within a receive")),
        ExecuteMsg::Deposit {
            token
        } => {
            let funds = funds.require_some(&token)?;
            deposit(storage, state, sender, funds)
         }
        _ => panic!("Not implemented yet"),
    }
}

fn deposit(
    storage: &mut dyn Storage,
    state: State,
    sender: Addr,
    funds: NonZero<Collateral>,
 ) -> Result<Response> {
    // let sender_shares = crate::state::SHARES
    //     .may_load(storage, &sender)
    //     .context("Could not load old shares")?
    //     .map(NonZero::raw)
    //     .unwrap_or_default();
    // let mut totals = crate::state::TOTALS
    //     .may_load(storage)
    //     .context("Could not load old total shares")?
    //     .unwrap_or_default();
    // let position_info = PositionsInfo::load(&state, &market)?;
    // let new_shares = totals.add_collateral(funds, &position_info)?;
    // let sender_shares = new_shares.checked_add(sender_shares)?;
    // crate::state::SHARES.save(storage, (&sender, &market.id), &sender_shares)?;
    // crate::state::REVERSE_SHARES.save(storage, (&market.id, &sender), &())?;
    // crate::state::TOTALS.save(storage, &market.id, &totals)?;

    // Ok(Response::new().add_event(
    //     Event::new("deposit")
    //         .add_attribute("lp", &sender)
    //         .add_attribute("collateral", funds.to_string())
    //         .add_attribute("new-shares", new_shares.to_string()),
    // ))
     todo!()
}
