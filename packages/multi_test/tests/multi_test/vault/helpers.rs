use anyhow::*;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, Coin, Deps, DepsMut, Empty, Env, MessageInfo, Response, Uint128,
};
use cw_multi_test::{App, AppBuilder, ContractWrapper, Executor};
use cw_storage_plus::Map;
use perpswap::contracts::vault::InstantiateMsg;
use perpswap::storage::{MarketExecuteMsg, MarketQueryMsg};
use perpswap::token::Token;
use vault;

pub fn setup_standard_vault(initial_balance: Option<Coin>) -> Result<(App, Addr, Addr)> {
    let (mut app, vault_addr) =
        setup_vault_contract("governance", "usdc", vec![5000, 5000], initial_balance)?;
    let market_addr = setup_market_contract(&mut app)?;
    Ok((app, vault_addr, market_addr))
}

pub fn setup_vault_contract(
    governance: &str,
    usdc_denom: &str,
    markets_allocation_bps: Vec<u16>,
    initial_balance: Option<Coin>,
) -> Result<(App, Addr)> {
    let mut app = AppBuilder::new().build(|_, _, _| {});

    if let Some(coin) = initial_balance {
        app.init_modules(|router, _, store| {
            router
                .bank
                .init_balance(store, &Addr::unchecked("sender"), vec![coin.clone()])
        })?;

        app.send_tokens(
            Addr::unchecked("sender"),
            Addr::unchecked(governance),
            &[coin.clone()],
        )?;
    }

    let code = ContractWrapper::new(vault::execute, vault::instantiate, vault::query);
    let code_id = app.store_code(Box::new(code));

    let instantiate_msg = InstantiateMsg {
        governance: governance.to_string(),
        markets_allocation_bps,
        usdc_denom: usdc_denom.to_string(),
    };
    let contract_addr = app.instantiate_contract(
        code_id,
        Addr::unchecked(governance),
        &instantiate_msg,
        &[],
        "Vault",
        None,
    )?;

    Ok((app, contract_addr))
}

pub fn setup_market_contract(app: &mut App) -> Result<Addr> {
    static MOCK_MARKET_LP: Map<&Addr, Uint128> = Map::new("lp_balances");
    static MOCK_MARKET_XLP: Map<&Addr, Uint128> = Map::new("xlp_balances");

    #[derive(serde::Serialize, serde::Deserialize)]
    struct StatusResp {
        liquidity: Liquidity,
        collateral: Token,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Liquidity {
        total_lp: Uint128,
        total_xlp: Uint128,
    }

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
                        denom: "usdc".to_string(),
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
                    .find(|&c| c.denom == "usdc")
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
