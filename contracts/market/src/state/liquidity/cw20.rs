/*
 * This is essentially a copy/paste/adapt from the reference cw20 spec
 * there are a few overall concepts:
 *
 * 1. Execute messages *always* get sent from the proxy contract
 * 2. Query messages can come from anywhere - but they'll usually be proxied too
 * 3. Multiple liquidity token kinds are supported (LP, xLP, etc.)
 * 4. Everything which is needed to satisfy the CW20 spec is stored in this module
 * 5. However we only really care about a subset dealing with balances
 * 6. We care much less about gas costs for proxied calls than core market operations
 * 7. Only expose mutable functions that are genuinely necessary for the other market modules
 */

use crate::prelude::*;
use cosmwasm_std::{Binary, Order, QueryResponse, Uint128};
use cw_storage_plus::{Bound, Map};
use cw_utils::Expiration;
use perpswap::{
    contracts::{
        cw20::{
            entry::{
                AllAccountsResponse, AllAllowancesResponse, AllSpenderAllowancesResponse,
                AllowanceInfo, AllowanceResponse, BalanceResponse, MarketingInfoResponse,
                SpenderAllowanceInfo, TokenInfoResponse,
            },
            events::{AllowanceChangeEvent, AllowanceChangeKind, SendEvent, TransferEvent},
            Cw20ReceiveMsg, ReceiverExecuteMsg,
        },
        liquidity_token::{
            entry::{ExecuteMsg, QueryMsg},
            LiquidityTokenKind,
        },
    },
    token::Token,
};

const LP_ALLOWANCES: Map<(&Addr, &Addr), AllowanceResponse> = Map::new(namespace::LP_ALLOWANCES);
const LP_ALLOWANCES_SPENDER: Map<(&Addr, &Addr), AllowanceResponse> =
    Map::new(namespace::LP_ALLOWANCES_SPENDER);
const XLP_ALLOWANCES: Map<(&Addr, &Addr), AllowanceResponse> = Map::new(namespace::XLP_ALLOWANCES);
const XLP_ALLOWANCES_SPENDER: Map<(&Addr, &Addr), AllowanceResponse> =
    Map::new(namespace::XLP_ALLOWANCES_SPENDER);

// settings for pagination
const DEFAULT_LIMIT: u32 = 10;
const DECIMAL_PLACES: u8 = 6;

impl State<'_> {
    pub(crate) fn liquidity_token_handle_query(
        &self,
        store: &dyn Storage,
        kind: LiquidityTokenKind,
        msg: QueryMsg,
    ) -> Result<QueryResponse> {
        match msg {
            QueryMsg::Kind {} => kind.query_result(),

            QueryMsg::Balance { address } => self
                .liquidity_token_balance(store, &address.validate(self.api)?, kind)?
                .query_result(),

            QueryMsg::TokenInfo {} => {
                let name = format!(
                    "{}-{}",
                    self.market_id(store)?,
                    match kind {
                        LiquidityTokenKind::Lp => "LEVLP",
                        LiquidityTokenKind::Xlp => "LEVxLP",
                    }
                );
                let liquidity_stats = self.load_liquidity_stats(store)?;
                TokenInfoResponse {
                    name: name.clone(),
                    symbol: name,
                    decimals: DECIMAL_PLACES,
                    total_supply: lp_token_into_uint_128(match kind {
                        LiquidityTokenKind::Lp => liquidity_stats.total_lp,
                        LiquidityTokenKind::Xlp => liquidity_stats.total_xlp,
                    })?,
                }
                .query_result()
            }

            QueryMsg::Allowance { owner, spender } => self
                .liquidity_token_allowance(
                    store,
                    &owner.validate(self.api)?,
                    &spender.validate(self.api)?,
                    kind,
                )?
                .query_result(),

            QueryMsg::AllAllowances {
                owner,
                start_after,
                limit,
            } => self
                .liquidity_token_owner_allowances(
                    store,
                    owner.validate(self.api)?,
                    start_after.map(|x| x.validate(self.api)).transpose()?,
                    limit,
                    kind,
                )?
                .query_result(),

            QueryMsg::AllSpenderAllowances {
                spender,
                start_after,
                limit,
            } => self
                .liquidity_token_spender_allowances(
                    store,
                    spender.validate(self.api)?,
                    start_after.map(|x| x.validate(self.api)).transpose()?,
                    limit,
                    kind,
                )?
                .query_result(),

            QueryMsg::AllAccounts { start_after, limit } => self
                .liquidity_token_all_accounts(
                    store,
                    start_after.map(|x| x.validate(self.api)).transpose()?,
                    limit,
                )?
                .query_result(),

            QueryMsg::MarketingInfo {} => MarketingInfoResponse::default().query_result(),

            QueryMsg::Version {} => {
                perp_bail!(
                    ErrorId::InvalidLiquidityTokenMsg,
                    ErrorDomain::LiquidityToken,
                    "unreachable (version msg)"
                );
            }
        }
    }

    fn liquidity_token_balance(
        &self,
        store: &dyn Storage,
        addr: &Addr,
        kind: LiquidityTokenKind,
    ) -> Result<BalanceResponse> {
        let amount = match kind {
            LiquidityTokenKind::Lp => self.lp_info(store, addr)?.lp_amount,
            // For xLP tokens, we do _not_ use the lp_info method, since that
            // includes xLP in the process of being unstaked, and those balances
            // cannot be transferred. Instead, we return the raw stored xLP
            // amount, which represents how much xLP will be left after
            // completing the current unstaking process.
            LiquidityTokenKind::Xlp => self.load_liquidity_stats_addr_default(store, addr)?.xlp,
        };
        let balance = lp_token_into_uint_128(amount)?;
        Ok(BalanceResponse { balance })
    }

    fn liquidity_token_allowance(
        &self,
        store: &dyn Storage,
        owner: &Addr,
        spender: &Addr,
        kind: LiquidityTokenKind,
    ) -> Result<AllowanceResponse> {
        let allowance = allowances_map(kind)
            .may_load(store, (owner, spender))?
            .unwrap_or_default();

        Ok(allowance)
    }

    fn liquidity_token_owner_allowances(
        &self,
        store: &dyn Storage,
        owner: Addr,
        start_after: Option<Addr>,
        limit: Option<u32>,
        kind: LiquidityTokenKind,
    ) -> Result<AllAllowancesResponse> {
        let limit = usize::try_from(limit.unwrap_or(DEFAULT_LIMIT).min(QUERY_MAX_LIMIT))?;
        let start: Option<Bound<&Addr>> = start_after.as_ref().map(Bound::exclusive);

        let allowances = allowances_map(kind)
            .prefix(&owner)
            .range(store, start, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                item.map(|(spender, allow)| AllowanceInfo {
                    spender,
                    allowance: allow.allowance,
                    expires: allow.expires,
                })
                .map_err(|err| err.into())
            })
            .collect::<Result<_>>()?;

        Ok(AllAllowancesResponse { allowances })
    }

    fn liquidity_token_spender_allowances(
        &self,
        store: &dyn Storage,
        spender: Addr,
        start_after: Option<Addr>,
        limit: Option<u32>,
        kind: LiquidityTokenKind,
    ) -> Result<AllSpenderAllowancesResponse> {
        let limit = usize::try_from(limit.unwrap_or(DEFAULT_LIMIT).min(QUERY_MAX_LIMIT))?;
        let start: Option<Bound<&Addr>> = start_after.as_ref().map(Bound::exclusive);

        let allowances = allowances_spender_map(kind)
            .prefix(&spender)
            .range(store, start, None, Order::Ascending)
            .take(limit)
            .map(|item| {
                item.map(|(owner, allow)| SpenderAllowanceInfo {
                    owner,
                    allowance: allow.allowance,
                    expires: allow.expires,
                })
                .map_err(|err| err.into())
            })
            .collect::<Result<_>>()?;
        Ok(AllSpenderAllowancesResponse { allowances })
    }

    fn liquidity_token_all_accounts(
        &self,
        store: &dyn Storage,
        start_after: Option<Addr>,
        limit: Option<u32>,
    ) -> Result<AllAccountsResponse> {
        let limit = usize::try_from(limit.unwrap_or(DEFAULT_LIMIT).min(QUERY_MAX_LIMIT))?;

        let accounts = self.liquidity_providers(store, start_after.as_ref(), limit)?;

        Ok(AllAccountsResponse { accounts })
    }

    //********* CORE FUNCTIONS *****************/
    pub(crate) fn liquidity_token_handle_exec(
        &self,
        ctx: &mut StateContext,
        msg_sender: Addr,
        kind: LiquidityTokenKind,
        msg: ExecuteMsg,
    ) -> Result<()> {
        match msg {
            ExecuteMsg::Transfer { recipient, amount } => {
                self.liquidity_token_transfer(
                    ctx,
                    msg_sender,
                    recipient.validate(self.api)?,
                    amount,
                    kind,
                )?;
            }

            ExecuteMsg::Send {
                contract,
                amount,
                msg,
            } => {
                self.liquidity_token_send_with_msg(
                    ctx,
                    msg_sender,
                    contract.validate(self.api)?,
                    amount,
                    msg,
                    kind,
                )?;
            }

            ExecuteMsg::IncreaseAllowance {
                spender,
                amount,
                expires,
            } => {
                self.liquidity_token_increase_allowance(
                    ctx,
                    msg_sender,
                    spender.validate(self.api)?,
                    amount,
                    expires,
                    kind,
                )?;
            }

            ExecuteMsg::DecreaseAllowance {
                spender,
                amount,
                expires,
            } => {
                self.liquidity_token_decrease_allowance(
                    ctx,
                    msg_sender,
                    spender.validate(self.api)?,
                    amount,
                    expires,
                    kind,
                )?;
            }

            ExecuteMsg::TransferFrom {
                owner,
                recipient,
                amount,
            } => {
                self.liquidity_token_transfer_from(
                    ctx,
                    msg_sender,
                    owner.validate(self.api)?,
                    recipient.validate(self.api)?,
                    amount,
                    kind,
                )?;
            }

            ExecuteMsg::SendFrom {
                owner,
                contract,
                amount,
                msg,
            } => {
                self.liquidity_token_send_with_msg_from(
                    ctx,
                    msg_sender,
                    owner.validate(self.api)?,
                    contract.validate(self.api)?,
                    amount,
                    msg,
                    kind,
                )?;
            }
        }

        Ok(())
    }

    //********* CW20 HANDLERS *****************/
    fn liquidity_token_transfer(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        recipient: Addr,
        amount: Uint128,
        kind: LiquidityTokenKind,
    ) -> Result<()> {
        ctx.response_mut().add_event(TransferEvent {
            owner: owner.clone(),
            recipient: recipient.clone(),
            amount,
            by: None,
        });

        let amount = uint_128_into_lp_token(amount.u128())?.into_signed();

        self.liquidity_token_balance_change_inner(ctx, &owner, -amount, kind)?;
        self.liquidity_token_balance_change_inner(ctx, &recipient, amount, kind)?;

        Ok(())
    }

    fn liquidity_token_transfer_from(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        owner: Addr,
        recipient: Addr,
        amount: Uint128,
        kind: LiquidityTokenKind,
    ) -> Result<()> {
        ctx.response_mut().add_event(TransferEvent {
            owner: owner.clone(),
            recipient: recipient.clone(),
            amount,
            by: Some(sender.clone()),
        });

        // deduct allowance before doing anything else have enough allowance
        self.liquidity_token_deduct_allowance_inner(ctx, &owner, &sender, amount, kind)?;

        let amount = uint_128_into_lp_token(amount.u128())?.into_signed();

        self.liquidity_token_balance_change_inner(ctx, &owner, -amount, kind)?;
        self.liquidity_token_balance_change_inner(ctx, &recipient, amount, kind)?;
        Ok(())
    }

    fn liquidity_token_send_with_msg(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        contract: Addr,
        amount: Uint128,
        msg: Binary,
        kind: LiquidityTokenKind,
    ) -> Result<()> {
        ctx.response_mut().add_event(SendEvent {
            owner: owner.clone(),
            contract: contract.clone(),
            amount,
            by: None,
        });

        ctx.response_mut().add_execute_submessage_oneshot(
            contract.clone(),
            &ReceiverExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: owner.to_string(),
                amount,
                msg,
            }),
        )?;

        // move the tokens to the contract
        let amount = uint_128_into_lp_token(amount.u128())?.into_signed();

        self.liquidity_token_balance_change_inner(ctx, &owner, -amount, kind)?;
        self.liquidity_token_balance_change_inner(ctx, &contract, amount, kind)?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn liquidity_token_send_with_msg_from(
        &self,
        ctx: &mut StateContext,
        sender: Addr,
        owner: Addr,
        contract: Addr,
        amount: Uint128,
        msg: Binary,
        kind: LiquidityTokenKind,
    ) -> Result<()> {
        ctx.response_mut().add_event(SendEvent {
            owner: owner.clone(),
            contract: contract.clone(),
            amount,
            by: Some(sender.clone()),
        });

        // deduct allowance before doing anything else have enough allowance
        self.liquidity_token_deduct_allowance_inner(ctx, &owner, &sender, amount, kind)?;

        ctx.response_mut().add_execute_submessage_oneshot(
            contract.clone(),
            &ReceiverExecuteMsg::Receive(Cw20ReceiveMsg {
                sender: sender.to_string(),
                amount,
                msg,
            }),
        )?;

        // move the tokens to the contract
        let amount = uint_128_into_lp_token(amount.u128())?.into_signed();

        self.liquidity_token_balance_change_inner(ctx, &owner, -amount, kind)?;
        self.liquidity_token_balance_change_inner(ctx, &contract, amount, kind)?;

        Ok(())
    }

    fn liquidity_token_increase_allowance(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        spender: Addr,
        amount: Uint128,
        expires: Option<Expiration>,
        kind: LiquidityTokenKind,
    ) -> Result<()> {
        ctx.response_mut().add_event(AllowanceChangeEvent {
            kind: AllowanceChangeKind::Increase,
            owner: owner.clone(),
            spender: spender.clone(),
            amount,
            expires,
        });

        if spender == owner {
            return Err(anyhow!("cannot increase allowance to own account"));
        }

        let update_fn = |allow: Option<AllowanceResponse>| -> Result<_> {
            let mut val = allow.unwrap_or_default();
            if let Some(exp) = expires {
                val.expires = exp;
            }
            val.allowance += amount;
            Ok(val)
        };
        allowances_map(kind).update(ctx.storage, (&owner, &spender), update_fn)?;
        allowances_spender_map(kind).update(ctx.storage, (&spender, &owner), update_fn)?;

        Ok(())
    }

    fn liquidity_token_decrease_allowance(
        &self,
        ctx: &mut StateContext,
        owner: Addr,
        spender: Addr,
        amount: Uint128,
        expires: Option<Expiration>,
        kind: LiquidityTokenKind,
    ) -> Result<()> {
        ctx.response_mut().add_event(AllowanceChangeEvent {
            kind: AllowanceChangeKind::Decrease,
            owner: owner.clone(),
            spender: spender.clone(),
            amount,
            expires,
        });

        if spender == owner {
            return Err(anyhow!("cannot decrease allowance to own account"));
        }

        let key = (&owner, &spender);

        fn reverse<'a>(t: (&'a Addr, &'a Addr)) -> (&'a Addr, &'a Addr) {
            (t.1, t.0)
        }

        // load value and delete if it hits 0, or update otherwise
        let mut allowance = allowances_map(kind).load(ctx.storage, key)?;
        if amount < allowance.allowance {
            // update the new amount
            allowance.allowance = allowance.allowance.checked_sub(amount)?;

            if let Some(exp) = expires {
                allowance.expires = exp;
            }
            allowances_map(kind).save(ctx.storage, key, &allowance)?;
            allowances_spender_map(kind).save(ctx.storage, reverse(key), &allowance)?;
        } else {
            allowances_map(kind).remove(ctx.storage, key);
            allowances_spender_map(kind).remove(ctx.storage, reverse(key));
        }

        Ok(())
    }

    // this can be used to update a lower allowance - call bucket.update with proper keys
    fn liquidity_token_deduct_allowance_inner(
        &self,
        ctx: &mut StateContext,
        owner: &Addr,
        spender: &Addr,
        amount: Uint128,
        kind: LiquidityTokenKind,
    ) -> Result<AllowanceResponse> {
        let block = self.env.block.clone();

        let update_fn = |current: Option<AllowanceResponse>| -> _ {
            match current {
                Some(mut a) => {
                    if a.expires.is_expired(&block) {
                        Err(anyhow!("Allowance is expired"))
                    } else {
                        // deduct the allowance if enough
                        a.allowance = a.allowance.checked_sub(amount)?;
                        Ok(a)
                    }
                }
                None => Err(anyhow!("no allowance")),
            }
        };
        allowances_map(kind).update(ctx.storage, (owner, spender), update_fn)?;
        allowances_spender_map(kind).update(ctx.storage, (spender, owner), update_fn)
    }

    fn liquidity_token_balance_change_inner(
        &self,
        ctx: &mut StateContext,
        owner: &Addr,
        delta: Signed<LpToken>,
        kind: LiquidityTokenKind,
    ) -> Result<()> {
        self.perform_lp_book_keeping(ctx, owner)?;

        let mut addr_stats = self.load_liquidity_stats_addr_default(ctx.storage, owner)?;

        // Negative delta means we're transferring our tokens somewhere else, so
        // check if we're in cooldown. Transferring xLP tokens is always fine
        // since they're time-locked.
        if delta.is_negative() && kind == LiquidityTokenKind::Lp {
            self.ensure_liquidity_cooldown(&addr_stats)?;
        }

        let m = match kind {
            LiquidityTokenKind::Lp => &mut addr_stats.lp,
            LiquidityTokenKind::Xlp => &mut addr_stats.xlp,
        };
        let new_balance = Signed::from(*m).checked_add(delta)?;
        *m = match new_balance.try_into_non_negative_value() {
            None => {
                return Err(anyhow!(
                    "balance for {owner} cannot be less than zero (tried to add {delta} to {m})",
                ))
            }
            Some(new_balance) => new_balance,
        };
        self.save_liquidity_stats_addr(ctx.storage, owner, &addr_stats)
    }
}

fn lp_token_into_uint_128(amount: LpToken) -> Result<Uint128> {
    // token already contains all the logic to convert. Won't actually reach out anywhere
    let token = Token::Cw20 {
        addr: String::new().into(),
        decimal_places: DECIMAL_PLACES,
    };
    Ok(token
        .into_u128(amount.into_decimal256())?
        .unwrap_or_default()
        .into())
}

fn uint_128_into_lp_token(amount: u128) -> Result<LpToken> {
    // token already contains all the logic to convert. Won't actually reach out anywhere
    let token = Token::Cw20 {
        addr: String::new().into(),
        decimal_places: DECIMAL_PLACES,
    };
    token.from_u128(amount).map(LpToken::from_decimal256)
}

fn allowances_map(
    kind: LiquidityTokenKind,
) -> Map<(&'static Addr, &'static Addr), AllowanceResponse> {
    match kind {
        LiquidityTokenKind::Lp => LP_ALLOWANCES,
        LiquidityTokenKind::Xlp => XLP_ALLOWANCES,
    }
}
fn allowances_spender_map(
    kind: LiquidityTokenKind,
) -> Map<(&'static Addr, &'static Addr), AllowanceResponse> {
    match kind {
        LiquidityTokenKind::Lp => LP_ALLOWANCES_SPENDER,
        LiquidityTokenKind::Xlp => XLP_ALLOWANCES_SPENDER,
    }
}
