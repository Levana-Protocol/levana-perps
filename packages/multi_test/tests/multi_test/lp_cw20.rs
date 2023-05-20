use levana_perpswap_multi_test::{
    contracts::cw20_receive::{MockCw20ReceiverContract, Payload as MockCw20Payload},
    market_wrapper::PerpsMarket,
    PerpsApp,
};
use msg::{contracts::liquidity_token::LiquidityTokenKind, prelude::*};

#[test]
fn lp_token_cw20_send() {
    let app = PerpsApp::new_cell().unwrap();
    let market = PerpsMarket::new(app.clone()).unwrap();
    let contract = MockCw20ReceiverContract::new(app).unwrap();
    let lp = Addr::unchecked("alice");
    let lp_token_addr = market
        .query_liquidity_token_addr(LiquidityTokenKind::Lp)
        .unwrap();

    market
        .exec_mint_and_deposit_liquidity(&lp, "1000000".parse().unwrap())
        .unwrap();

    market
        .exec_liquidity_token_send(
            LiquidityTokenKind::Lp,
            &lp,
            &contract.addr,
            "5".parse().unwrap(),
            &MockCw20Payload::Print {
                value: "hello world".to_string(),
                enforce_sender: Some(lp.to_string()),
                enforce_info_sender: Some(lp_token_addr.to_string()),
            },
        )
        .unwrap();

    assert_eq!(
        market.query_lp_info(&lp).unwrap().lp_amount,
        "999995".parse().unwrap()
    );
}

#[test]
fn lp_token_cw20_send_from() {
    let app = PerpsApp::new_cell().unwrap();
    let market = PerpsMarket::new(app.clone()).unwrap();
    let contract = MockCw20ReceiverContract::new(app).unwrap();
    let lp = Addr::unchecked("alice");
    let otherlp = Addr::unchecked("bob");
    let lp_token_addr = market
        .query_liquidity_token_addr(LiquidityTokenKind::Lp)
        .unwrap();

    market
        .exec_mint_and_deposit_liquidity(&lp, "1000000".parse().unwrap())
        .unwrap();

    // try to send without allowance, fails
    market
        .exec_liquidity_token_send_from(
            LiquidityTokenKind::Lp,
            &otherlp,
            &lp,
            &contract.addr,
            "500".parse().unwrap(),
            &MockCw20Payload::Print {
                value: "hello world".to_string(),
                enforce_sender: None,
                enforce_info_sender: None,
            },
        )
        .unwrap_err();

    // give allowance
    market
        .exec_liquidity_token_increase_allowance(
            LiquidityTokenKind::Lp,
            &lp,
            &otherlp,
            "500".parse().unwrap(),
        )
        .unwrap();

    // trying to send more than given allowance fails
    market
        .exec_liquidity_token_send_from(
            LiquidityTokenKind::Lp,
            &otherlp,
            &lp,
            &contract.addr,
            "600".parse().unwrap(),
            &MockCw20Payload::Print {
                value: "hello world".to_string(),
                enforce_sender: None,
                enforce_info_sender: None,
            },
        )
        .unwrap_err();

    // sending fails if we think it's coming from "lp" instead of "otherlp" on behalf of "lp"
    market
        .exec_liquidity_token_send_from(
            LiquidityTokenKind::Lp,
            &otherlp,
            &lp,
            &contract.addr,
            "500".parse().unwrap(),
            &MockCw20Payload::Print {
                value: "hello world".to_string(),
                enforce_sender: Some(lp.to_string()),
                enforce_info_sender: Some(lp_token_addr.to_string()),
            },
        )
        .unwrap_err();

    // sending the full allowance succeeds - and we can enforce the sender checks strictly
    market
        .exec_liquidity_token_send_from(
            LiquidityTokenKind::Lp,
            &otherlp,
            &lp,
            &contract.addr,
            "500".parse().unwrap(),
            &MockCw20Payload::Print {
                value: "hello world".to_string(),
                enforce_sender: Some(otherlp.to_string()),
                enforce_info_sender: Some(lp_token_addr.to_string()),
            },
        )
        .unwrap();

    // no allowance left
    market
        .exec_liquidity_token_send_from(
            LiquidityTokenKind::Lp,
            &otherlp,
            &lp,
            &contract.addr,
            "1".parse().unwrap(),
            &MockCw20Payload::Print {
                value: "hello world".to_string(),
                enforce_sender: None,
                enforce_info_sender: None,
            },
        )
        .unwrap_err();

    // lp balance is changed
    assert_eq!(
        market.query_lp_info(&lp).unwrap().lp_amount,
        "999500".parse().unwrap()
    );
}
