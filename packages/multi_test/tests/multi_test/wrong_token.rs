use cosmwasm_std::{testing::MockApi, to_json_binary};
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{contracts::liquidity_token::LiquidityTokenKind, prelude::*, token::Token};

#[test]
fn directly_call_receive() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let mock_api = MockApi::default();

    let addr = mock_api.addr_make("notacw20");
    let fakesender = mock_api.addr_make("fakesender");
    let err: PerpError = market
        .exec(
            &addr,
            &MarketExecuteMsg::Receive {
                sender: fakesender.into(),
                amount: 1000000000u128.into(),
                msg: to_json_binary(&MarketExecuteMsg::DepositLiquidity {
                    stake_to_xlp: false,
                })
                .unwrap(),
            },
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err.id, ErrorId::Cw20Funds);
}

#[test]
fn deposit_lp_token() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let mock_api = MockApi::default();
    let lp = mock_api.addr_make("provider");
    let other_address = mock_api.addr_make("other-address");

    market
        .exec_mint_and_deposit_liquidity(&lp, "1000000".parse().unwrap())
        .unwrap();

    // Confirm that a basic transfer works
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Lp,
            &lp,
            &other_address,
            "5".parse().unwrap(),
        )
        .unwrap();

    let err: PerpError = market
        .exec_liquidity_token_send(
            LiquidityTokenKind::Lp,
            &lp,
            &market.addr,
            "5".parse().unwrap(),
            &MarketExecuteMsg::DepositLiquidity {
                stake_to_xlp: false,
            },
        )
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err.id, ErrorId::Cw20Funds);

    // And confirm that a basic transfer works still, ruling out insufficient liquidity
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Lp,
            &lp,
            &other_address,
            "5".parse().unwrap(),
        )
        .unwrap();
}

#[test]
fn unneeded_funds() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&trader, "1000000".parse().unwrap())
        .unwrap();

    market
        .exec_funds(
            &trader,
            &MarketExecuteMsg::WithdrawLiquidity { lp_amount: None },
            "10".parse().unwrap(),
        )
        .unwrap_err();
}

#[test]
fn unrelated_native() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();
    market
        .exec_mint_tokens(&trader, "1000".parse().unwrap())
        .unwrap();

    market
        .exec_mint_native(&trader, "usomethingelse", 1_000_000u64)
        .unwrap();

    // We can only attach native funds for the market contract in non-CW20 markets.
    if let Token::Native { .. } = &market.token {
        let msg = match market
            .make_market_msg_with_funds(
                &MarketExecuteMsg::DepositLiquidity {
                    stake_to_xlp: false,
                },
                "100".parse().unwrap(),
            )
            .unwrap()
        {
            cosmwasm_std::WasmMsg::Execute {
                contract_addr,
                msg,
                mut funds,
            } => {
                funds.push(cosmwasm_std::Coin {
                    denom: "usomethingelse".to_owned(),
                    amount: 1_000_000u64.into(),
                });
                cosmwasm_std::WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds,
                }
            }
            _ => panic!("Expected an Execute"),
        };
        market.exec_wasm_msg(&trader, msg).unwrap_err();
    }
}
