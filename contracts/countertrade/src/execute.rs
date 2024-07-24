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

    fn require_some(self, market: &MarketInfo) -> Result<NonZero<Collateral>> {
        match self {
            Funds::NoFunds => Err(anyhow!(
                "Message requires attached funds, but none were provided"
            )),
            Funds::Funds { token, amount } => {
                token.ensure_matches(&market.token)?;
                let collateral = market
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
        ExecuteMsg::Deposit { market } => {
            let market = state.load_cache_market_info(storage, &market)?;
            let funds = funds.require_some(&market)?;
            deposit(storage, state, sender, funds, market)
        }
        ExecuteMsg::Withdraw { amount, market } => {
            funds.require_none()?;
            let market = state.load_cache_market_info(storage, &market)?;
            withdraw(storage, state, sender, market, amount)
        }
        ExecuteMsg::DoWork { market } => {
            funds.require_none()?;
            let market = state.load_cache_market_info(storage, &market)?;
            crate::work::execute(storage, state, market)
        }
        ExecuteMsg::AppointAdmin { admin } => {
            funds.require_none()?;
            let admin = admin.validate(state.api)?;
            appoint_admin(state, storage, sender, admin)
        }
        ExecuteMsg::AcceptAdmin {} => {
            funds.require_none()?;
            accept_admin(state, storage, sender)
        }
        ExecuteMsg::UpdateConfig(config_update) => {
            funds.require_none()?;
            update_config(state, storage, sender, config_update)
        }
    }
}

fn deposit(
    storage: &mut dyn Storage,
    state: State,
    sender: Addr,
    funds: NonZero<Collateral>,
    market: MarketInfo,
) -> Result<Response> {
    let sender_shares = crate::state::SHARES
        .may_load(storage, (&sender, &market.id))
        .context("Could not load old shares")?
        .map(NonZero::raw)
        .unwrap_or_default();
    let mut totals = crate::state::TOTALS
        .may_load(storage, &market.id)
        .context("Could not load old total shares")?
        .unwrap_or_default();
    let position_info = PositionsInfo::load(&state, &market)?;
    let new_shares = totals.add_collateral(funds, &position_info)?;
    let sender_shares = new_shares.checked_add(sender_shares)?;
    crate::state::SHARES.save(storage, (&sender, &market.id), &sender_shares)?;
    crate::state::REVERSE_SHARES.save(storage, (&market.id, &sender), &())?;
    crate::state::TOTALS.save(storage, &market.id, &totals)?;

    Ok(Response::new().add_event(
        Event::new("deposit")
            .add_attribute("lp", &sender)
            .add_attribute("collateral", funds.to_string())
            .add_attribute("new-shares", new_shares.to_string()),
    ))
}

fn withdraw(
    storage: &mut dyn Storage,
    state: State,
    sender: Addr,
    market: MarketInfo,
    amount: NonZero<LpToken>,
) -> Result<Response> {
    let sender_shares = crate::state::SHARES
        .may_load(storage, (&sender, &market.id))
        .context("Could not load old shares")?
        .map(NonZero::raw)
        .unwrap_or_default();
    ensure!(
        sender_shares >= amount.raw(),
        "Insufficient shares. You have {sender_shares}, but tried to withdraw {amount}"
    );
    let mut totals = crate::state::TOTALS
        .may_load(storage, &market.id)
        .context("Could not load old total shares")?
        .unwrap_or_default();
    let position_info = PositionsInfo::load(&state, &market)?;
    let collateral = totals.remove_collateral(amount, &position_info)?;
    let sender_shares = sender_shares.checked_sub(amount.raw())?;
    match NonZero::new(sender_shares) {
        None => {
            crate::state::REVERSE_SHARES.remove(storage, (&market.id, &sender));
            crate::state::SHARES.remove(storage, (&sender, &market.id))
        }
        Some(sender_shares) => {
            crate::state::REVERSE_SHARES.save(storage, (&market.id, &sender), &())?;
            crate::state::SHARES.save(storage, (&sender, &market.id), &sender_shares)?
        }
    }
    crate::state::TOTALS.save(storage, &market.id, &totals)?;

    let collateral =
        NonZero::new(collateral).context("Action would result in 0 collateral transferred")?;
    let msg = market
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
            .add_event(Event::new("appoint-admin").add_attribute("new-admin", new_admin)),
    )
}

fn accept_admin(mut state: State, storage: &mut dyn Storage, sender: Addr) -> Result<Response> {
    ensure!(
        state.config.pending_admin.as_ref() == Some(&sender),
        "Cannot accept admin, you're not currently the pending admin"
    );
    let old_admin = state.config.admin;
    state.config.admin = sender.clone();
    state.config.pending_admin = None;
    crate::state::CONFIG.save(storage, &state.config)?;
    Ok(Response::new().add_event(
        Event::new("accept-admin")
            .add_attribute("old-admin", old_admin)
            .add_attribute("new-admin", sender),
    ))
}

fn update_config(
    mut state: State,
    storage: &mut dyn Storage,
    sender: Addr,
    ConfigUpdate {
        min_funding,
        target_funding,
        max_funding,
        max_leverage,
    }: ConfigUpdate,
) -> Result<Response> {
    ensure!(
        state.config.admin == sender,
        "You are not the admin, you cannot update the config"
    );

    let mut event = Event::new("update-config");

    if let Some(min_funding) = min_funding {
        event = event.add_attribute("old-min-funding", state.config.min_funding.to_string());
        event = event.add_attribute("new-min-funding", min_funding.to_string());
        state.config.min_funding = min_funding;
    }

    if let Some(target_funding) = target_funding {
        event = event.add_attribute(
            "old-target-funding",
            state.config.target_funding.to_string(),
        );
        event = event.add_attribute("new-target-funding", target_funding.to_string());
        state.config.target_funding = target_funding;
    }

    if let Some(max_funding) = max_funding {
        event = event.add_attribute("old-max-funding", state.config.max_funding.to_string());
        event = event.add_attribute("new-max-funding", max_funding.to_string());
        state.config.max_funding = max_funding;
    }

    if let Some(max_leverage) = max_leverage {
        event = event.add_attribute("old-max-funding", state.config.max_leverage.to_string());
        event = event.add_attribute("new-max-funding", max_leverage.to_string());
        state.config.max_leverage = max_leverage;
    }

    crate::state::CONFIG.save(storage, &state.config)?;

    Ok(Response::new().add_event(event))
}
