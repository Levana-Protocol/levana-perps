use std::str::FromStr;

use cosmwasm_std::{Addr, Decimal256};
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::countertrade::{ConfigUpdate, HasWorkResp, MarketBalance, WorkDescription},
    prelude::{DirectionToBase, Number, TakeProfitTrader, UnsignedDecimal, Usd},
    shared::number::{Collateral, NonZero},
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
    // Still wants to balance the market, but won't succeed fully
    assert_ne!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    // Now we run the countertrade contract, which will open a small position
    // Trying to run again will result in no work.
    do_work(&market, &lp);
    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    // And if we try to add more liquidity and try again, it should
    // update the existing position
    market
        .exec_countertrade_mint_and_deposit(&lp, "1000")
        .unwrap();
    // Updates the position now
    do_work(&market, &lp);
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
                msg::prelude::MarketType::CollateralIsQuote => "6",
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

        // Don't collect so that we can test the intermediate states
        do_work_optional_collect(&market, &lp, false);

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
                    direction,
                    leverage: _,
                    collateral: _,
                    take_profit: _,
                    stop_loss_override: _,
                },
        } => {
            assert_eq!(direction, DirectionToBase::Short)
        }
        has_work => panic!("Unexpected has work response: {has_work:?}"),
    }

    do_work(&market, &lp);

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
                "0.0001".parse().unwrap()
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
    do_work_optional_collect(market, lp, true)
}

fn do_work_optional_collect(market: &PerpsMarket, lp: &Addr, collect_closed: bool) {
    let work = market.query_countertrade_has_work().unwrap();
    let (has_deferred_exec, is_close) = match work {
        HasWorkResp::NoWork {} => panic!("do_work when no work is available"),
        HasWorkResp::Work { desc } => match desc {
            WorkDescription::OpenPosition { .. } => (true, false),
            WorkDescription::ClosePosition { .. } => (true, true),
            WorkDescription::CollectClosedPosition { .. } => {
                panic!("CollectClosedPosition in do_work")
            }
            WorkDescription::ResetShares => (false, false),
            WorkDescription::ClearDeferredExec { .. } => panic!("ClearDeferredExec in do_work"),
            WorkDescription::UpdatePositionAddCollateralImpactSize { .. } => (true, false),
            WorkDescription::UpdatePositionRemoveCollateralImpactSize { .. } => (true, false),
        },
    };
    market.exec_countertrade_do_work().unwrap();
    // Will fail if we do more work again since the deferred
    // execution would not have finished
    market.exec_countertrade_do_work().unwrap_err();

    // Execute the deferred message
    market.exec_crank_till_finished(lp).unwrap();
    // And clear any deferred exec IDs and collect any closed positions

    if has_deferred_exec {
        match market.query_countertrade_has_work().unwrap() {
            HasWorkResp::Work {
                desc: WorkDescription::ClearDeferredExec { id: _ },
            } => (),
            work => panic!("Unexpected work response: {work:?}"),
        }
        market.exec_countertrade_do_work().unwrap();
    }

    if is_close {
        match market.query_countertrade_has_work().unwrap() {
            HasWorkResp::Work {
                desc: WorkDescription::CollectClosedPosition { .. },
            } => (),
            work => panic!("Unexpected work response: {work:?}"),
        }
        if collect_closed {
            market.exec_countertrade_do_work().unwrap();
        }
    }
}

#[test]
fn deduct_balance() {
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

    let balance = market
        .query_countertrade_balances(&lp)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        balance.collateral,
        NonZero::new(Collateral::from_str("100").unwrap()).unwrap()
    );

    match market.query_countertrade_has_work().unwrap() {
        HasWorkResp::Work {
            desc:
                WorkDescription::OpenPosition {
                    direction: DirectionToBase::Short,
                    collateral,
                    ..
                },
        } => {
            let pos_collateral = match market_type {
                msg::shared::storage::MarketType::CollateralIsQuote => {
                    Collateral::from_str("1.615376150827128342").unwrap()
                }
                msg::shared::storage::MarketType::CollateralIsBase => {
                    Collateral::from_str("1.468523773479207584").unwrap()
                }
            };

            assert_eq!(collateral.raw(), pos_collateral);
        }
        has_work => panic!("Unexpected has_work: {has_work:?}"),
    }

    market.exec_countertrade_do_work().unwrap();

    // Calculate before deferred execution so that DNF fee doesn't
    // influence the available total
    let balance = market
        .query_countertrade_balances(&lp)
        .unwrap()
        .pop()
        .unwrap();
    // 100 - 1.46 = 98.54, 100 - 1.61 = 98.39
    match market_type {
        msg::shared::storage::MarketType::CollateralIsQuote => assert!(balance
            .collateral
            .raw()
            .approx_eq(Collateral::from_str("98.384623849").unwrap())),
        msg::shared::storage::MarketType::CollateralIsBase => assert!(balance
            .collateral
            .raw()
            .approx_eq(Collateral::from_str("98.531476226").unwrap())),
    }
}

#[test]
fn update_position_scenario_add_collateral() {
    let market = make_countertrade_market().unwrap();
    let lp = market.clone_lp(0).unwrap();
    let trader = market.clone_trader(0).unwrap();

    assert_eq!(
        market.query_countertrade_has_work().unwrap(),
        HasWorkResp::NoWork {}
    );

    let countertrade_config = market.query_countertrade_config().unwrap();

    // Make sure there are funds to open a position
    market
        .exec_countertrade_mint_and_deposit(&lp, "100")
        .unwrap();

    let market_type = market.query_status().unwrap().market_type;

    // Open up unbalanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "7",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();

    // Execute the deferred message
    market.exec_crank_till_finished(&lp).unwrap();

    let status = market.query_status().unwrap();
    assert!(status.long_notional > status.short_notional);
    do_work(&market, &lp);
    let status = market.query_status().unwrap();
    let countertrade_position = market
        .query_countertrade_market_id(status.market_id)
        .unwrap()
        .position
        .unwrap();
    assert_eq!(
        countertrade_position.direction_to_base,
        DirectionToBase::Short
    );
    market
        .exec_open_position_take_profit(
            &trader,
            "1",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "5",
                msg::prelude::MarketType::CollateralIsBase => "10",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();

    market.exec_crank_till_finished(&lp).unwrap();
    let status = market.query_status().unwrap();

    assert!(status
        .long_funding
        .approx_eq_eps("0.9".parse().unwrap(), "0.05".parse().unwrap())
        .unwrap());

    let work = market.query_countertrade_has_work().unwrap();
    match work {
        HasWorkResp::NoWork {} => panic!("impossible: expected work"),
        HasWorkResp::Work { ref desc } => match desc {
            WorkDescription::UpdatePositionAddCollateralImpactSize { pos_id, .. } => {
                assert_eq!(countertrade_position.id, pos_id.clone());
            }
            desc => panic!("Got invalid work: {desc}"),
        },
    }

    do_work(&market, &lp);
    let updated_position = market
        .query_countertrade_market_id(status.market_id)
        .unwrap()
        .position
        .unwrap();
    assert!(countertrade_position.deposit_collateral < updated_position.deposit_collateral);

    let status = market.query_status().unwrap();
    assert!(status
        .long_funding
        .approx_eq_eps(
            countertrade_config.target_funding.into_number(),
            "0.1".parse().unwrap()
        )
        .unwrap());
}

#[test]
fn update_position_scenario_remove_collateral() {
    let market = make_countertrade_market().unwrap();
    // Set minimum_deposit_usd so that countertrade countract tries to
    // reduce the collateral instead of closing the position.
    market
        .exec_set_config(msg::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();
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

    let market_type = market.query_status().unwrap().market_type;

    // Open up unbalanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "20",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "7",
                msg::prelude::MarketType::CollateralIsBase => "7",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();

    // Execute the deferred message
    market.exec_crank_till_finished(&lp).unwrap();
    let status = market.query_status().unwrap();
    assert!(status.long_notional > status.short_notional);
    do_work(&market, &lp);

    let status = market.query_status().unwrap();

    let countertrade_position = market
        .query_countertrade_market_id(status.market_id)
        .unwrap()
        .position
        .unwrap();
    assert_eq!(
        countertrade_position.direction_to_base,
        DirectionToBase::Short
    );
    let status = market.query_status().unwrap();
    // Popular position is still long_funding
    assert!(status.long_funding.is_strictly_positive());

    // This flip the popular side from Long to Short
    market
        .exec_open_position_take_profit(
            &trader,
            "20",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "3",
                msg::prelude::MarketType::CollateralIsBase => "3",
            },
            DirectionToBase::Short,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("0.9".parse().unwrap()),
        )
        .unwrap();

    market.exec_crank_till_finished(&lp).unwrap();
    let status = market.query_status().unwrap();

    // Short position is the popular one
    assert!(status.short_funding.is_strictly_positive());

    let work = market.query_countertrade_has_work().unwrap();
    match work {
        HasWorkResp::NoWork {} => panic!("impossible: expected work"),
        HasWorkResp::Work { ref desc } => match desc {
            WorkDescription::UpdatePositionRemoveCollateralImpactSize { pos_id, .. } => {
                assert_eq!(countertrade_position.id, pos_id.clone());
            }
            desc => panic!("Got invalid work: {desc}"),
        },
    };
    do_work(&market, &lp);
    let status = market.query_status().unwrap();
    let updated_position = market
        .query_countertrade_market_id(status.market_id)
        .unwrap()
        .position
        .unwrap();

    // Collateral has reduced for the countertrade position
    assert!(updated_position.deposit_collateral < countertrade_position.deposit_collateral);
}

#[test]
fn regression_perp_4098() {
    let market = make_countertrade_market().unwrap();
    // Bump up the iteration limit
    market
        .exec_countertrade_update_config(ConfigUpdate {
            iterations: Some(150),
            ..Default::default()
        })
        .unwrap();

    // Set minimum_deposit_usd so that countertrade countract tries to
    // reduce the collateral instead of closing the position.
    market
        .exec_set_config(msg::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();
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

    let market_type = market.query_status().unwrap().market_type;

    // Open up unbalanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "12",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "7",
                msg::prelude::MarketType::CollateralIsBase => "8",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();

    // Execute the deferred message
    market.exec_crank_till_finished(&lp).unwrap();
    let status = market.query_status().unwrap();
    assert!(status.long_notional > status.short_notional);
    do_work(&market, &lp);

    let status = market.query_status().unwrap();

    let countertrade_position = market
        .query_countertrade_market_id(status.market_id)
        .unwrap()
        .position
        .unwrap();
    assert_eq!(
        countertrade_position.direction_to_base,
        DirectionToBase::Short
    );
    let status = market.query_status().unwrap();
    // Popular position is still long_funding
    assert!(status.long_funding.is_strictly_positive());

    // This flip the popular side from Long to Short
    market
        .exec_open_position_take_profit(
            &trader,
            "20",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "3",
                msg::prelude::MarketType::CollateralIsBase => "3",
            },
            DirectionToBase::Short,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("0.9".parse().unwrap()),
        )
        .unwrap();

    market.exec_crank_till_finished(&lp).unwrap();
    let status = market.query_status().unwrap();

    // Short position is the popular one
    assert!(status.short_funding.is_strictly_positive());
    println!("status.short_funding: {}", status.short_funding);
    println!("status.long_funding: {}", status.long_funding);

    let work = market.query_countertrade_has_work().unwrap();
    match work {
        HasWorkResp::NoWork {} => panic!("impossible: expected work"),
        HasWorkResp::Work { ref desc } => match desc {
            WorkDescription::UpdatePositionRemoveCollateralImpactSize { pos_id, .. } => {
                assert_eq!(countertrade_position.id, pos_id.clone());
            }
            desc => panic!("Got invalid work: {desc}"),
        },
    };
    do_work(&market, &lp);
    let status = market.query_status().unwrap();
    let updated_position = market
        .query_countertrade_market_id(status.market_id)
        .unwrap()
        .position
        .unwrap();

    // Collateral has reduced for the countertrade position
    assert!(updated_position.deposit_collateral < countertrade_position.deposit_collateral);
}

#[test]
fn do_not_mutate_countertrade_position() {
    let market = make_countertrade_market().unwrap();
    // Set minimum_deposit_usd so that countertrade countract tries to
    // reduce the collateral instead of closing the position.
    market
        .exec_set_config(msg::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();
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

    let market_type = market.query_status().unwrap().market_type;

    // Open up unbalanced positions
    market
        .exec_open_position_take_profit(
            &trader,
            "10",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "7",
                msg::prelude::MarketType::CollateralIsBase => "7",
            },
            DirectionToBase::Long,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("1.1".parse().unwrap()),
        )
        .unwrap();

    // Execute the deferred message
    market.exec_crank_till_finished(&lp).unwrap();

    do_work(&market, &lp);

    let status = market.query_status().unwrap();

    let countertrade_position = market
        .query_countertrade_market_id(status.market_id)
        .unwrap()
        .position
        .unwrap();
    assert_eq!(
        countertrade_position.direction_to_base,
        DirectionToBase::Short
    );
    let status = market.query_status().unwrap();
    // Popular position is still long_funding
    assert!(status.long_funding.is_strictly_positive());

    // This flip the popular side from Long to Short. But does it so
    // that it doesn't make sense for countertrade contract to reduce
    // the position.
    market
        .exec_open_position_take_profit(
            &trader,
            "40",
            // Deal with off-by-one leverage to ensure we have a balanced market
            match market_type {
                msg::prelude::MarketType::CollateralIsQuote => "3",
                msg::prelude::MarketType::CollateralIsBase => "1",
            },
            DirectionToBase::Short,
            None,
            None,
            msg::prelude::TakeProfitTrader::Finite("0.9".parse().unwrap()),
        )
        .unwrap();

    market.exec_crank_till_finished(&lp).unwrap();
    let status = market.query_status().unwrap();

    // Short position is the popular one
    assert!(status.short_funding.is_strictly_positive());

    let work = market.query_countertrade_has_work().unwrap();
    match work {
        HasWorkResp::NoWork {} => panic!("impossible: expected work"),
        HasWorkResp::Work { ref desc } => match desc {
            WorkDescription::ClosePosition { pos_id } => {
                assert_eq!(countertrade_position.id, pos_id.clone());
            }
            desc => panic!("Got invalid work: {desc}"),
        },
    };
    do_work(&market, &lp);
}
