use anyhow::*;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, Coin, Deps, DepsMut, Empty, Env, MessageInfo, Response, Uint128,
};
use cw_multi_test::{App, AppBuilder, ContractWrapper, Executor};
use cw_storage_plus::Map;
use perpswap::contracts::vault::{InstantiateMsg, UsdcAssetInit};
use perpswap::storage::{MarketExecuteMsg, MarketQueryMsg};
use perpswap::token::Token;
use std::collections::HashMap;

pub const GOVERNANCE: &str = "cosmwasm1h72z9g4qf2kjrq866zgn78xl32wn0q8aqayp05jkjpgdp2qft5aquanhrh";
pub const USER: &str = "cosmwasm1qnufjmd8vwm6j6d3q28wxqr4d8408f34fpka4vs365fvskualrasv5ues5";
pub const USER1: &str = "cosmwasm1vqjarrly327529599rcc4qhzvhwe34pp5uyy4gylvxe5zupeqx3sg08lap";
pub const USDC: &str = "usdc";

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StatusResp {
    pub liquidity: Liquidity,
    pub collateral: Token,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Liquidity {
    pub total_lp: Uint128,
    pub total_xlp: Uint128,
}

fn build_markets_allocations_bps(
    app: &mut App,
    markets_allocation_bps: Vec<u16>,
) -> Result<HashMap<String, u16>> {
    markets_allocation_bps
        .into_iter()
        .map(|bps| {
            let market = setup_market_contract(app)?;
            Ok((market.to_string(), bps))
        })
        .collect()
}

pub fn init_user_balance(app: &mut App, user: &str, amount: u128) -> Result<()> {
    let coin = Coin::new(amount, USDC);
    app.init_modules(|router, _, store| {
        router
            .bank
            .init_balance(store, &Addr::unchecked(user), vec![coin.clone()])
    })?;
    Ok(())
}

pub fn setup_vault_contract(
    markets_allocation_bps: Vec<u16>,
    initial_balance: Option<Coin>,
) -> Result<(App, Addr, Vec<Addr>)> {
    let mut app = AppBuilder::new().build(|_, _, _| {});

    let funds = match initial_balance.clone() {
        Some(coin) => {
            app.init_modules(|router, _, store| {
                router
                    .bank
                    .init_balance(store, &Addr::unchecked(GOVERNANCE), vec![coin.clone()])
            })?;
            &[coin].to_vec()
        }
        None => &vec![],
    };

    let code = ContractWrapper::new(vault::execute, vault::instantiate, vault::query);
    let code_id = app.store_code(Box::new(code));

    let markets_allocation_bps = build_markets_allocations_bps(&mut app, markets_allocation_bps)?;

    let instantiate_msg = InstantiateMsg {
        usdc_denom: UsdcAssetInit::Native {
            denom: (USDC.to_owned()),
        },
        governance: GOVERNANCE.to_string(),
        markets_allocation_bps: markets_allocation_bps.clone(),
    };

    let contract_addr = app.instantiate_contract(
        code_id,
        Addr::unchecked(GOVERNANCE),
        &instantiate_msg,
        funds,
        "Vault",
        Some(GOVERNANCE.to_string()),
    )?;

    Ok((
        app,
        contract_addr,
        markets_allocation_bps.keys().map(Addr::unchecked).collect(),
    ))
}

pub fn setup_market_contract(app: &mut App) -> Result<Addr> {
    static MOCK_MARKET_LP: Map<&Addr, Uint128> = Map::new("lp_balances");
    static MOCK_MARKET_XLP: Map<&Addr, Uint128> = Map::new("xlp_balances");

    fn query(deps: Deps, _env: Env, msg: MarketQueryMsg) -> Result<Binary> {
        match msg {
            MarketQueryMsg::Status { .. } => {
                let total_lp = MOCK_MARKET_LP
                    .may_load(deps.storage, &Addr::unchecked("vault"))
                    .unwrap()
                    .unwrap_or(Uint128::zero());
                let total_xlp = MOCK_MARKET_XLP
                    .may_load(deps.storage, &Addr::unchecked("vault"))
                    .unwrap()
                    .unwrap_or(Uint128::zero());
                Ok(to_json_binary(&StatusResp {
                    liquidity: Liquidity {
                        total_lp,
                        total_xlp,
                    },
                    collateral: Token::Native {
                        denom: USDC.to_string(),
                        decimal_places: 6,
                    },
                })?)
            }
            _ => unimplemented!(),
        }
    }

    fn execute(
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        msg: MarketExecuteMsg,
    ) -> Result<Response> {
        match msg {
            MarketExecuteMsg::DepositLiquidity { stake_to_xlp } => {
                let amount = info
                    .funds
                    .iter()
                    .find(|&c| c.denom == USDC)
                    .map(|c| c.amount)
                    .unwrap_or(Uint128::zero());

                let key = Addr::unchecked("vault");
                if stake_to_xlp {
                    MOCK_MARKET_XLP.update(deps.storage, &key, |old| {
                        Ok(old.unwrap_or(Uint128::zero()) + amount)
                    })?;
                } else {
                    MOCK_MARKET_LP.update(deps.storage, &key, |old| {
                        Ok(old.unwrap_or(Uint128::zero()) + amount)
                    })?;
                }
                Ok(Response::new().add_attribute("action", "deposit_liquidity"))
            }
            MarketExecuteMsg::ClaimYield {} => {
                Ok(Response::new().add_attribute("action", "claim_yield"))
            }
            MarketExecuteMsg::WithdrawLiquidity {
                lp_amount,
                claim_yield: _,
            } => {
                let key = Addr::unchecked("vault");
                let lp_token = lp_amount
                    .ok_or_else(|| anyhow!("No LP amount provided"))?
                    .raw();
                let amount = Uint128::from(lp_token.into_u128()?);
                MOCK_MARKET_LP.update(deps.storage, &key, |old| {
                    let old = old.unwrap_or(Uint128::zero());
                    if old < amount {
                        return Err(anyhow!("Insufficient LP"));
                    }
                    Ok(old - amount)
                })?;
                Ok(Response::new().add_attribute("action", "withdraw_liquidity"))
            }
            _ => unimplemented!(),
        }
    }

    fn default_instantiate(
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _msg: Empty,
    ) -> Result<Response> {
        Ok(Response::default())
    }

    let code = ContractWrapper::new(execute, default_instantiate, query);
    let code_id = app.store_code(Box::new(code));

    Ok(app.instantiate_contract(
        code_id,
        Addr::unchecked("admin"),
        &Empty {},
        &[],
        "MockMarket",
        None,
    )?)
}
