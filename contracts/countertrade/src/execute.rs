use crate::{prelude::*, types::PositionsInfo};

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
                Err(anyhow!("Unnecessary funds sent: {amount}{token}"))
            }
        }
    }

    fn require_some(self, market_state: &MarketState) -> Result<NonZero<Collateral>> {
        match self {
            Funds::NoFunds => Err(anyhow!(
                "Message requires attached funds, but none were provided"
            )),
            Funds::Funds { token, amount } => {
                match (&token, &market_state.market.token) {
                    (
                        Token::Native(_),
                        msg::token::Token::Cw20 {
                            addr,
                            ..
                        },
                    ) => bail!("Provided native funds, but market requires a CW20 (contract {addr})"),
                    (
                        Token::Native(denom1),
                        msg::token::Token::Native {
                            denom:denom2,
                            decimal_places:_,
                        },
                    ) => ensure!(denom1 == denom2, "Wrong denom provided. You sent {denom1}, but the contract expects {denom2}"),
                    (
                        Token::Cw20(addr1),
                        msg::token::Token::Cw20 {
                            addr:addr2,
                            decimal_places:_,
                        },
                    ) => ensure!(addr1.as_str() == addr2.as_str(), "Wrong CW20 used. You used {addr1}, but the contract expects {addr2}"),
                    (
                        Token::Cw20(_),
                        msg::token::Token::Native {
                            denom,
                            ..
                        },
                    ) => bail!("Provided CW20 funds, but market requires native funds with denom {denom}"),
                }
                let collateral = market_state
                    .market
                    .token
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
                    "Cannot attached funds when performing a CW20 receive"
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
pub fn execute(deps: DepsMut, _env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    let HandleFunds { funds, msg, sender } = handle_funds(deps.api, info, msg)?;
    match msg {
        ExecuteMsg::Receive { .. } => Err(anyhow!("Cannot perform a receive within a receive")),
        ExecuteMsg::Deposit { market } => {
            let (market_state, storage) = MarketState::load_mut(deps, market)?;
            let funds = funds.require_some(&market_state)?;
            deposit(storage, sender, funds, market_state)
        }
        ExecuteMsg::Withdraw { amount, market } => {
            funds.require_none()?;
            let (market_state, storage) = MarketState::load_mut(deps, market)?;
            withdraw(storage, sender, market_state, amount)
        }
        ExecuteMsg::Crank { market } => todo!(),
        ExecuteMsg::AppointAdmin { admin } => {
            funds.require_none()?;
            let state = State::load(deps.api, deps.querier, deps.storage)?;
            let admin = admin.validate(deps.api)?;
            appoint_admin(state, deps.storage, sender, admin)
        }
        ExecuteMsg::AcceptAdmin {} => {
            funds.require_none()?;
            let state = State::load(deps.api, deps.querier, deps.storage)?;
            accept_admin(state, deps.storage, sender)
        }
        ExecuteMsg::UpdateConfig(_) => todo!(),
    }
}

fn deposit(
    storage: &mut dyn Storage,
    sender: Addr,
    funds: NonZero<Collateral>,
    market_state: MarketState,
) -> Result<Response> {
    let sender_shares = crate::state::SHARES
        .may_load(storage, (&sender, &market_state.market.id))
        .context("Could not load old shares")?
        .map(NonZero::raw)
        .unwrap_or_default();
    let mut totals = crate::state::TOTALS
        .may_load(storage, &market_state.market.id)
        .context("Could not load old total shares")?
        .unwrap_or_default();
    let position_info = PositionsInfo::load();
    let new_shares = totals.add_collateral(funds, &position_info)?;
    let sender_shares = new_shares.checked_add(sender_shares)?;
    crate::state::SHARES.save(storage, (&sender, &market_state.market.id), &sender_shares)?;
    crate::state::TOTALS.save(storage, &market_state.market.id, &totals)?;

    Ok(Response::new().add_event(
        Event::new("deposit")
            .add_attribute("lp", &sender)
            .add_attribute("collateral", funds.to_string())
            .add_attribute("new-shares", new_shares.to_string()),
    ))
}

fn withdraw(
    storage: &mut dyn Storage,
    sender: Addr,
    market_state: MarketState,
    amount: NonZero<LpToken>,
) -> Result<Response> {
    let sender_shares = crate::state::SHARES
        .may_load(storage, (&sender, &market_state.market.id))
        .context("Could not load old shares")?
        .map(NonZero::raw)
        .unwrap_or_default();
    ensure!(
        sender_shares >= amount.raw(),
        "Insufficient shares. You have {sender_shares}, but tried to withdraw {amount}"
    );
    let mut totals = crate::state::TOTALS
        .may_load(storage, &market_state.market.id)
        .context("Could not load old total shares")?
        .unwrap_or_default();
    let position_info = PositionsInfo::load();
    let collateral = totals.remove_collateral(amount, &position_info)?;
    let sender_shares = sender_shares.checked_sub(amount.raw())?;
    match NonZero::new(sender_shares) {
        None => crate::state::SHARES.remove(storage, (&sender, &market_state.market.id)),
        Some(sender_shares) => crate::state::SHARES.save(
            storage,
            (&sender, &market_state.market.id),
            &sender_shares,
        )?,
    }
    crate::state::TOTALS.save(storage, &market_state.market.id, &totals)?;

    let collateral =
        NonZero::new(collateral).context("Action would result in 0 collateral transferred")?;
    let msg = market_state
        .market
        .token
        .into_transfer_msg(&sender, collateral)?
        .context("Collateral amount would be less than the chain's minimum representation")?;

    Ok(Response::new()
        .add_event(
            Event::new("withdraw")
                .add_attribute("lp", &sender)
                .add_attribute("collateral", collateral.to_string())
                .add_attribute("burned-shares", amount.to_string()),
        )
        .add_message(msg))
}

fn appoint_admin(
    mut state: State,
    storage: &mut dyn Storage,
    sender: Addr,
    new_admin: Addr,
) -> Result<Response> {
    ensure!(
        state.config.admin == sender,
        "You are not the admin, you cannot appoint a new admin"
    );
    state.config.pending_admin = Some(new_admin.clone());
    crate::state::CONFIG.save(storage, &state.config)?;
    Ok(
        Response::new()
            .add_event(Event::new("appoint_admin").add_attribute("new-admin", new_admin)),
    )
}

fn accept_admin(mut state: State, storage: &mut dyn Storage, sender: Addr) -> Result<Response> {
    ensure!(
        state.config.pending_admin.as_ref() == Some(&sender),
        "Cannot accept admin, you're not currently the pending admin"
    );
    let old_admin = std::mem::replace(&mut state.config.admin, sender.clone());
    state.config.pending_admin = None;
    crate::state::CONFIG.save(storage, &state.config)?;
    Ok(Response::new().add_event(
        Event::new("accept_admin")
            .add_attribute("old-admin", old_admin)
            .add_attribute("new-admin", sender),
    ))
}
