use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{
    config::{SpotPriceKind, TokenKind, DEFAULT_MARKET, TEST_CONFIG},
    market_wrapper::PerpsMarket,
    time::TimeJump,
    PerpsApp,
};
use perpswap::{
    storage::{DirectionToBase, MarketId},
    token::TokenInit,
};

#[test]
fn instantiate_price_early_3025() {
    let app = PerpsApp::new_cell().unwrap();

    let now = app.borrow().block_info().time;
    let before_instantiation_time = now.minus_seconds(20);

    let token_init = match DEFAULT_MARKET.token_kind {
        TokenKind::Native => TokenInit::Native {
            denom: TEST_CONFIG.native_denom.to_string(),
            decimal_places: 6,
        },
        TokenKind::Cw20 => {
            let addr = app
                .borrow_mut()
                .get_cw20_addr(&DEFAULT_MARKET.cw20_symbol)
                .unwrap();
            TokenInit::Cw20 { addr: addr.into() }
        }
    };
    let market = PerpsMarket::new_custom(
        app,
        MarketId::new(
            DEFAULT_MARKET.base.clone(),
            DEFAULT_MARKET.quote.clone(),
            DEFAULT_MARKET.collateral_type,
        ),
        token_init,
        DEFAULT_MARKET.initial_price,
        None,
        // use an old timestamp for initial price publish time
        Some(before_instantiation_time.into()),
        // and do not attempt to bootstrap lps
        false,
        SpotPriceKind::Oracle,
    )
    .unwrap();

    // this will fail, due to the fix in this issue (cannot use a spot price publish time earlier than instantiation time)
    // note that prior to this fix, it would have succeeded - and thereby bricked the protocol
    // since it would push an invalid price into the crank queue that could then never be moved forward
    market
        .exec_crank_n(&Addr::unchecked("init-cranker"), 0)
        .unwrap_err();

    // also fails to initialize lps
    market
        .exec_mint_and_deposit_liquidity(
            &DEFAULT_MARKET.bootstrap_lp_addr,
            DEFAULT_MARKET.bootstrap_lp_deposit,
        )
        .unwrap_err();

    // in fact we can't even query the price - because there is none
    market.query_current_price().unwrap_err();

    // but, if we push a new price
    market.exec_set_price(DEFAULT_MARKET.initial_price).unwrap();

    //  everything works

    market
        .exec_mint_and_deposit_liquidity(
            &DEFAULT_MARKET.bootstrap_lp_addr,
            DEFAULT_MARKET.bootstrap_lp_deposit,
        )
        .unwrap();
    market.query_current_price().unwrap();

    let trader = market.clone_trader(0).unwrap();

    let queue_res = market
        .exec_open_position_queue_only(
            &trader,
            "100",
            "9",
            DirectionToBase::Long,
            "8",
            None,
            None,
            None,
        )
        .unwrap();

    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_refresh_price().unwrap();

    // success!
    market
        .exec_open_position_process_queue_response(&trader, queue_res, None)
        .unwrap();
}
