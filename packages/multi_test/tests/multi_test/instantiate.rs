use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{
    config::{SpotPriceKind, TokenKind, DEFAULT_MARKET, TEST_CONFIG},
    market_wrapper::PerpsMarket,
    time::TimeJump,
    PerpsApp,
};
use msg::{
    shared::storage::{DirectionToBase, MarketId},
    token::TokenInit,
};

#[test]
fn instantiate_price_early_3025() {
    let app = PerpsApp::new_cell().unwrap();

    let now = app.borrow().block_info().time;
    let before_instantiation_time = now.minus_hours(1);

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

    // cranking will fail due to not having a valid price
    market
        .exec_crank_n(&Addr::unchecked("init-cranker"), 1)
        .unwrap_err();

    // as will initializing lps
    market
        .exec_mint_and_deposit_liquidity(
            &DEFAULT_MARKET.bootstrap_lp_addr,
            DEFAULT_MARKET.bootstrap_lp_deposit,
        )
        .unwrap_err();

    // in fact we can't even query the price
    market.query_current_price().unwrap_err();

    // but, we *can* push a new price now
    market.exec_set_price(DEFAULT_MARKET.initial_price).unwrap();

    // which opens up everything...

    market
        .exec_crank_n(&Addr::unchecked("init-cranker"), 1)
        .unwrap();
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
