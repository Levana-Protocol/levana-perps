use cosmwasm_std::{Addr, Decimal256};
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::countertrade::{ConfigUpdate, HasWorkResp, MarketBalance, WorkDescription},
    prelude::{DirectionToBase, Number, TakeProfitTrader, UnsignedDecimal, Usd},
};

fn make_countertrade_market() -> anyhow::Result<PerpsMarket> {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Remove minimum deposit so that we can open tiny balancing positions
    market.exec_set_config(msg::contracts::market::config::ConfigUpdate {
        minimum_deposit_usd: Some(Usd::zero()),
        ..Default::default()
    })?;
    Ok(market)
}

#[test]
fn query_config() {
    let market = make_countertrade_market().unwrap();

    market.query_countertrade_config().unwrap();
}

#[test]
fn deposit() {
    let market = make_countertrade_market().unwrap();
    let lp = market.clone_lp(0).unwrap();

    assert_eq!(market.query_countertrade_balances(&lp).unwrap(), vec![]);

    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();
    let mut balances = market.query_countertrade_balances(&lp).unwrap();
    assert_eq!(balances.len(), 1);
    let MarketBalance {
        market: _,
        token: _,
        shares,
        collateral,
        pool_size,
    } = balances.pop().unwrap();
    assert_eq!(shares.to_string(), "100");
    assert_eq!(collateral.to_string(), "100");
    assert_eq!(pool_size.to_string(), "100");

    let lp = market.clone_lp(1).unwrap();

    market
        .exec_countertrade_mint_and_deposit(&lp, "50")
        .unwrap();
    let mut balances = market.query_countertrade_balances(&lp).unwrap();
    assert_eq!(balances.len(), 1);
    let MarketBalance {
        market: _,
        token: _,
        shares,
        collateral,
        pool_size,
    } = balances.pop().unwrap();
    assert_eq!(shares.to_string(), "50");
    assert_eq!(collateral.to_string(), "50");
    assert_eq!(pool_size.to_string(), "150");
}

#[test]
fn withdraw_no_positions() {
    let market = make_countertrade_market().unwrap();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();

    market
        .exec_countertrade_mint_and_deposit(&lp0, "100")
        .unwrap();
    market
        .exec_countertrade_mint_and_deposit(&lp1, "100")
        .unwrap();

    let balance_before = market.query_collateral_balance(&lp0).unwrap();
    market.exec_countertrade_withdraw(&lp0, "50").unwrap();
    market.exec_countertrade_withdraw(&lp0, "51").unwrap_err();
    let balance_after = market.query_collateral_balance(&lp0).unwrap();
    let expected = balance_before.checked_add("50".parse().unwrap()).unwrap();
    assert_eq!(
        expected, balance_after,
        "Before: {balance_before}. After: {balance_after}. Expected after: {expected}"
    );

    let mut balances = market.query_countertrade_balances(&lp0).unwrap();
    assert_eq!(balances.len(), 1);
    let MarketBalance {
        market: _,
        token: _,
        shares,
        collateral,
        pool_size,
    } = balances.pop().unwrap();
    assert_eq!(shares.to_string(), "50");
    assert_eq!(collateral.to_string(), "50");
    assert_eq!(pool_size.to_string(), "150");

    let mut balances = market.query_countertrade_balances(&lp1).unwrap();
    assert_eq!(balances.len(), 1);
    let MarketBalance {
        market: _,
        token: _,
        shares,
        collateral,
        pool_size,
    } = balances.pop().unwrap();
    assert_eq!(shares.to_string(), "100");
    assert_eq!(collateral.to_string(), "100");
    assert_eq!(pool_size.to_string(), "150");
}

#[test]
fn change_admin() {
    let market = make_countertrade_market().unwrap();
    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();

    market.exec_countertrade_accept_admin(&lp0).unwrap_err();
    market.exec_countertrade_appoint_admin(&lp0).unwrap();
    market.exec_countertrade_accept_admin(&lp1).unwrap_err();
    market.exec_countertrade_appoint_admin(&lp1).unwrap();
    market.exec_countertrade_accept_admin(&lp0).unwrap_err();
    market.exec_countertrade_accept_admin(&lp1).unwrap();
    market.exec_countertrade_appoint_admin(&lp0).unwrap_err();

    let config = market.query_countertrade_config().unwrap();
    assert_eq!(config.admin, lp1);
    assert_eq!(config.pending_admin, None);
}

#[test]
fn update_config() {
    let market = make_countertrade_market().unwrap();
    let lp0 = market.clone_lp(0).unwrap();

    let min_funding: Decimal256 = "0.0314".parse().unwrap();

    assert_ne!(
        market.query_countertrade_config().unwrap().min_funding,
        min_funding
    );

    market
        .exec_countertrade_update_config(ConfigUpdate {
            min_funding: Some(min_funding),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(
        market.query_countertrade_config().unwrap().min_funding,
        min_funding
    );

    market.exec_countertrade_appoint_admin(&lp0).unwrap();
    market.exec_countertrade_accept_admin(&lp0).unwrap();

    market
        .exec_countertrade_update_config(ConfigUpdate {
            min_funding: Some("4".parse().unwrap()),
            ..Default::default()
        })
        .unwrap_err();

    assert_eq!(
        market.query_countertrade_config().unwrap().min_funding,
        min_funding
    );
}

#[test]
fn has_no_work() {
    let market = make_countertrade_market().unwrap();
    let lp = market.clone_lp(0).unwrap();
    let trader = market.clone_trader(0).unwrap();

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    let config = market.query_countertrade_config().unwrap();
    let market_type = market.query_status().unwrap().market_type;

    // Open up balanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "6",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "4",
            },
            DirectionToBase::Short,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("0.9".parse().unwrap()),
        )
        .unwrap();
    let status = market.query_status().unwrap();
    assert!(
        status.long_funding < config.max_funding.into_signed(),
        "Long funding rates are too high: {}. Need less than {}.",
        status.long_funding,
        config.max_funding
    );
    assert!(
        status.short_funding < config.max_funding.into_signed(),
        "Short funding rates are too high: {}. Need less than {}.",
        status.short_funding,
        config.max_funding
    );
    assert_eq!(status.long_funding, Number::zero());
    assert_eq!(status.short_funding, Number::zero());

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );
    // Executing when there's no work should fail
    market.exec_countertrade_do_work().unwrap_err();
}

#[test]
fn detects_unbalanced_markets() {
    let market = make_countertrade_market().unwrap();
    let lp = market.clone_lp(0).unwrap();
    let trader = market.clone_trader(0).unwrap();

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    // Make sure there are funds to open a position
    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();

    let config = market.query_countertrade_config().unwrap();
    let market_type = market.query_status().unwrap().market_type;

    // Open up unbalanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "6",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "3",
                msg::prelude::MarketType::CollateralIsBase => "2",
            },
            DirectionToBase::Short,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("0.9".parse().unwrap()),
        )
        .unwrap();
    let status = market.query_status().unwrap();
    assert!(
        status.long_funding > config.max_funding.into_signed(),
        "Long funding rates are not high enough: {}. Need greater than {}.",
        status.long_funding,
        config.max_funding
    );
    assert!(
        status.short_funding < config.max_funding.into_signed(),
        "Short funding rates are too high: {}. Need less than {}.",
        status.short_funding,
        config.max_funding
    );

    match market.query_countertrade_has_work().unwrap() {
        HasWorkResp::Work {
            desc:
                WorkDescription::OpenPosition {
                    direction: DirectionToBase::Short,
                    ..
                },
        } => (),
        has_work => panic!("Unexpected has_work: {has_work:?}"),
    }
}

#[test]
fn ignores_unbalanced_insufficient_liquidity() {
    let market = make_countertrade_market().unwrap();
    let trader = market.clone_trader(0).unwrap();

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    let config = market.query_countertrade_config().unwrap();
    let market_type = market.query_status().unwrap().market_type;

    // Open up unbalanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "6",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "3",
                msg::prelude::MarketType::CollateralIsBase => "2",
            },
            DirectionToBase::Short,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("0.9".parse().unwrap()),
        )
        .unwrap();
    let status = market.query_status().unwrap();
    assert!(
        status.long_funding > config.max_funding.into_signed(),
        "Long funding rates are not high enough: {}. Need greater than {}.",
        status.long_funding,
        config.max_funding
    );
    assert!(
        status.short_funding < config.max_funding.into_signed(),
        "Short funding rates are too high: {}. Need less than {}.",
        status.short_funding,
        config.max_funding
    );

    // Even though the market is unbalanced, we have no funds to open a position with.
    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    // This should fail when there's no work to do
    market.exec_countertrade_do_work().unwrap_err();

    // Put in a small amount of funds, less than the minimum
    let lp = market.clone_lp(0).unwrap();

    market
        .exec_countertrade_mint_and_deposit(&lp, "0.005")
        .unwrap();
    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );
}

#[test]
fn closes_extra_positions() {
    let market = make_countertrade_market().unwrap();
    let countertrade = market.get_countertrade_addr();
    let lp = market.clone_lp(0).unwrap();
    let market_type = market.query_status().unwrap().market_type;

    // Do a deposit to avoid confusing the contract. As an optimization, the contract
    // won't check if there are open positions if there is no liquidity deposited.
    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();

    market
        .exec_mint_tokens(&countertrade, "1000".parse().unwrap())
        .unwrap();
    // Force open positions as the contract
    let mut pos_ids = vec![];
    for _ in 0..5 {
        let (pos_id, _) = market
            .exec_open_position_take_profit(
                &countertrade,
                "10",
                "5",
                DirectionToBase::Long,
                None,
                None,
                msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
            )
            .unwrap();
        pos_ids.push(pos_id);
    }

    // And open a larger counterposition to make sure these positions are all unpopular
    market
        .exec_open_position_take_profit(
            &lp,
            "9",
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "4",
            },
            DirectionToBase::Short,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("0.9".parse().unwrap()),
        )
        .unwrap();

    for pos_id in pos_ids.into_iter().take(4) {
        // Get the status before we close the position, for comparison below
        let market_before = market.query_countertrade_markets().unwrap().pop().unwrap();
        let balance_before = market.query_collateral_balance(&countertrade).unwrap();

        // We should be forced to close the first open position
        assert_eq!(
            market.query_countertrade_has_work().unwrap(),
            HasWorkResp::Work {
                desc: msg::contracts::countertrade::WorkDescription::ClosePosition { pos_id }
            }
        );

        do_work(&market, &lp);

        // Position must be closed
        let pos = market.query_closed_position(&countertrade, pos_id).unwrap();

        // Determine the active collateral that will actually be transferred
        let active_collateral = market
            .token
            .round_down_to_precision(pos.active_collateral)
            .unwrap();

        // Ensure that now we want to collect the information from that closed position
        assert_eq!(
            market.query_countertrade_has_work().unwrap(),
            HasWorkResp::Work {
                desc: msg::contracts::countertrade::WorkDescription::CollectClosedPosition {
                    pos_id,
                    close_time: pos.close_time,
                    active_collateral
                }
            }
        );

        // Without collecting, our balances remain the same
        let market_before_work = market.query_countertrade_markets().unwrap().pop().unwrap();
        assert_eq!(market_before_work.collateral, market_before.collateral);

        // Now collect the balances
        market.exec_countertrade_do_work().unwrap();

        // And confirm the countertrade contract saw the update
        let market_after = market.query_countertrade_markets().unwrap().pop().unwrap();
        assert_eq!(
            Ok(market_after.collateral),
            market_before.collateral + active_collateral
        );

        // And finally confirm that the balance in the contract itself really changed
        let balance_after = market.query_collateral_balance(&countertrade).unwrap();
        assert_eq!(
            balance_after,
            (balance_before + active_collateral.into_number()).unwrap()
        );
    }

    // Nothing left to be done now
    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );
    market.exec_countertrade_do_work().unwrap_err();
}

#[test]
fn closes_popular_position_long() {
    closes_popular_position_helper(DirectionToBase::Long, true);
    closes_popular_position_helper(DirectionToBase::Long, false);
}

#[test]
fn closes_popular_position_short() {
    closes_popular_position_helper(DirectionToBase::Short, true);
    closes_popular_position_helper(DirectionToBase::Short, false);
}

fn closes_popular_position_helper(direction: DirectionToBase, open_unpop: bool) {
    let market = make_countertrade_market().unwrap();
    let countertrade = market.get_countertrade_addr();
    let lp = market.clone_lp(0).unwrap();

    // Do a deposit to avoid confusing the contract. As an optimization, the contract
    // won't check if there are open positions if there is no liquidity deposited.
    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();

    let (take_profit_pop, take_profit_unpop) = match direction {
        DirectionToBase::Long => ("1.1", "0.9"),
        DirectionToBase::Short => ("0.9", "1.1"),
    };
    let take_profit_pop = TakeProfitTrader::Finite(take_profit_pop.parse().unwrap());

    // Open a position on behalf of the contract
    market
        .exec_mint_tokens(&countertrade, "1000".parse().unwrap())
        .unwrap();
    let (pos_id, _) = market
        .exec_open_position_take_profit(
            &countertrade,
            "10",
            "5",
            direction,
            None,
            None,
            take_profit_pop,
        )
        .unwrap();

    // And follow it up with a position by the LP
    market
        .exec_open_position_take_profit(&lp, "100", "5", direction, None, None, take_profit_pop)
        .unwrap();
    if open_unpop {
        // And an unpopular position to allow funding rates to be calculated.
        market
            .exec_open_position_take_profit(
                &lp,
                "5",
                "5",
                direction.invert(),
                None,
                None,
                TakeProfitTrader::Finite(take_profit_unpop.parse().unwrap()),
            )
            .unwrap();
    }

    market.exec_crank_till_finished(&lp).unwrap();
    market
        .set_time(levana_perpswap_multi_test::time::TimeJump::Blocks(1))
        .unwrap();

    // We should need to close this position
    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::Work {
            desc: msg::contracts::countertrade::WorkDescription::ClosePosition { pos_id }
        }
    );

    // Sends a deferred message to close the position
    market.exec_countertrade_do_work().unwrap();
    // Execute the deferred message
    market.exec_crank_till_finished(&lp).unwrap();

    // Position must be closed
    market.query_closed_position(&countertrade, pos_id).unwrap();

    assert_eq!(
        market
            .query_countertrade_markets()
            .unwrap()
            .pop()
            .unwrap()
            .position,
        None
    );
}

#[test]
#[ignore]
#[allow(unreachable_code)]
fn resets_token_balances() {
    let market = make_countertrade_market().unwrap();
    let lp = market.clone_lp(0).unwrap();

    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();
    assert_ne!(market.query_countertrade_balances(&lp).unwrap(), vec![]);

    todo!("force a new position to get opened by the contract and then close it manually");
    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::Work {
            desc: msg::contracts::countertrade::WorkDescription::ResetShares
        }
    );
    assert_eq!(market.query_countertrade_balances(&lp).unwrap(), vec![]);
    market.exec_countertrade_do_work().unwrap();
    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );
    market.exec_countertrade_do_work().unwrap_err();
    assert_eq!(market.query_countertrade_balances(&lp).unwrap(), vec![]);
    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();
    assert_ne!(market.query_countertrade_balances(&lp).unwrap(), vec![]);
}

#[test]
fn opens_balancing_position() {
    let market = make_countertrade_market().unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    // Remove minimum deposit so that we can open tiny balancing positions
    market
        .exec_set_config(msg::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some(Usd::zero()),
            ..Default::default()
        })
        .unwrap();

    market
        .exec_countertrade_mint_and_deposit(&lp, "1000")
        .unwrap();

    let config = market.query_countertrade_config().unwrap();
    println!("config: {config:#?}");

    let market_type = market.query_status().unwrap().market_type;

    // Open up unbalanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "6",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "3",
                msg::prelude::MarketType::CollateralIsBase => "2",
            },
            DirectionToBase::Short,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("0.9".parse().unwrap()),
        )
        .unwrap();
    let status = market.query_status().unwrap();
    println!("before opening cc pos status: {status:#?}");
    assert!(
        status.long_funding > config.max_funding.into_signed(),
        "Long funding rates are not high enough: {}. Need greater than {}.",
        status.long_funding,
        config.max_funding
    );
    assert!(
        status.short_funding < config.max_funding.into_signed(),
        "Short funding rates are too high: {}. Need less than {}.",
        status.short_funding,
        config.max_funding
    );

    let price_point = market.query_current_price().unwrap();

    match market.query_countertrade_has_work().unwrap() {
        HasWorkResp::Work {
            desc:
                WorkDescription::OpenPosition {
                    direction,
                    leverage,
                    collateral,
                    take_profit: _,
                },
        } => {
            assert_eq!(direction, DirectionToBase::Short)
        }
        has_work => panic!("Unexpected has work response: {has_work:?}"),
    }

    do_work(&market, &lp);
    let status = market.query_status().unwrap();
    println!("after opening status: {status:#?}");

    let market_status = market
        .query_countertrade_market_id(status.market_id)
        .unwrap();

    let position = market_status.position.unwrap();

    println!("Opened position: {position:#?}");

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    let status = market.query_status().unwrap();
    assert!(
        status
            .long_funding
            .approx_eq_eps(
                config.target_funding.into_signed(),
                "0.00001".parse().unwrap()
            )
            .unwrap(),
        "Long funding {} should be close to target_funding {}",
        status.long_funding,
        config.target_funding
    );

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );
}

#[test]
fn balance_one_sided_market() {
    let market = make_countertrade_market().unwrap();
    let lp = market.clone_lp(0).unwrap();
    let trader = market.clone_trader(0).unwrap();

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    let status = market.query_status().unwrap();
    let market_type = status.market_type;

    // Open up balanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "6",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();

    let status = market.query_status().unwrap();

    assert_eq!(status.long_funding, Number::zero());
    assert_eq!(status.short_funding, Number::zero());

    assert_ne!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );
}

fn do_work(market: &PerpsMarket, lp: &Addr) {
    market.exec_countertrade_do_work().unwrap();
    // Will fail if we do more work again since the deferred
    // execution would not have finished
    market.exec_countertrade_do_work().unwrap_err();
    // Execute the deferred message
    market.exec_crank_till_finished(&lp).unwrap();
    // And clear out the deferred exec ID
    match market.query_countertrade_has_work().unwrap() {
        HasWorkResp::Work {
            desc: WorkDescription::ClearDeferredExec { id: _ },
        } => (),
        work => panic!("Unexpected work response: {work:?}"),
    }
    market.exec_countertrade_do_work().unwrap();
}
