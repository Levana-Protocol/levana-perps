use anyhow::{anyhow, ensure, Context, Result};
use msg::contracts::factory::entry::MarketsResp;

use crate::{
    prelude::*,
    types::{MarketInfo, State},
};

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
        ExecuteMsg::Deposit { token } => {
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
    todo!()
}

fn compute_lp_token_value(
    storage: &mut dyn Storage,
    state: State,
    token: Token,
) -> Result<Response> {
    // todo: track operations
    let token_value = crate::state::LP_TOKEN_VALUE
        .may_load(storage, &token)
        .context("Could not load LP_TOKEN_VALE")?
        .unwrap_or_default();
    let token_valid = token_value.status.valid();
    if token_valid {
        return Ok(Response::new());
    }
    let markets = state.load_market_ids_with_token(storage, token)?;
    for market in &markets {
        process_single_market(storage, &state, market)?;
    }
    validate_all_markets(storage, &state, &markets)?;
    // Calculate LP token value and update it
    todo!()
}

fn validate_all_markets(
    storage: &mut dyn Storage,
    state: &State<'_>,
    all_markets: &Vec<MarketInfo>,
) -> Result<()> {
    // Fetch all open position and validate that traked open positions isn't changed
    // Fetch all limit orders and validae that it isn't changed
    // If it changes, return error
    todo!()
}

fn process_single_market(
    storage: &mut dyn Storage,
    state: &State<'_>,
    market: &MarketInfo,
) -> Result<()> {
    // todo: track count of query operations!
    let mut market_work = crate::state::MARKET_WORK_INFO
        .may_load(storage, &market.id)
        .context("Could not load MARKET_WORK_INFO")?
        .unwrap_or_default();
    loop {
        let mut tokens_start_after = None;
        let tokens = state.load_tokens(&market.addr, tokens_start_after)?;
        tokens_start_after = tokens.start_after;
        // todo: optimize if empty tokens
        let positions = state.load_positions(&market.addr, tokens.tokens)?;
        let mut total_collateral = Collateral::zero();
        for position in positions {
            total_collateral = total_collateral.checked_add(position.active_collateral.raw())?;
            market_work.increment_open_position();
        }
        market_work.active_collateral = market_work
            .active_collateral
            .checked_add(total_collateral)?;
        if tokens_start_after.is_none() {
            break;
        }
        // Todo: Also break if query count exeeds!
    }
    crate::state::MARKET_WORK_INFO.save(storage, &market.id, &market_work);
    // Fetch all limit orders, track total limit order
    todo!()
}
