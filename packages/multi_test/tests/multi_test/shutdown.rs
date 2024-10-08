use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use perpswap::prelude::*;
use perpswap::shutdown::{ShutdownEffect, ShutdownImpact};

#[test]
fn shutdown_permissions() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // No permissions
    market
        .exec_shutdown(
            &Addr::unchecked("no-perms"),
            ShutdownEffect::Enable,
            &[],
            &[],
        )
        .unwrap_err();

    let kill_switch = Addr::unchecked(&TEST_CONFIG.kill_switch);
    let wind_down = Addr::unchecked(&TEST_CONFIG.wind_down);

    // Enabling as a no-op can be done by either kill switch or wind down
    for wallet in [&kill_switch, &wind_down] {
        market
            .exec_shutdown(wallet, ShutdownEffect::Enable, &[], &[])
            .unwrap();
    }

    // Market wind down has limited permissions, cannot shut it all down
    market
        .exec_shutdown(&wind_down, ShutdownEffect::Disable, &[], &[])
        .unwrap_err();

    // Kill switch can do everything
    market
        .exec_shutdown(&kill_switch, ShutdownEffect::Disable, &[], &[])
        .unwrap();
    market
        .exec_shutdown(&wind_down, ShutdownEffect::Enable, &[], &[])
        .unwrap_err();
    market
        .exec_shutdown(&kill_switch, ShutdownEffect::Enable, &[], &[])
        .unwrap();

    assert_eq!(market.query_shutdown_status(&market.id).unwrap(), vec![]);
    market
        .query_shutdown_status(&"FAKE_PAIR".parse().unwrap())
        .unwrap_err();

    market
        .exec_shutdown(
            &kill_switch,
            ShutdownEffect::Disable,
            &[],
            &[ShutdownImpact::NewTrades],
        )
        .unwrap();
    assert_eq!(
        market.query_shutdown_status(&market.id).unwrap(),
        vec![ShutdownImpact::NewTrades]
    );
    market
        .exec_shutdown(
            &wind_down,
            ShutdownEffect::Enable,
            &[&market.id],
            &[ShutdownImpact::NewTrades, ShutdownImpact::Crank],
        )
        .unwrap();
    assert_eq!(market.query_shutdown_status(&market.id).unwrap(), vec![]);
}

#[test]
fn shutdown_blocks_trades() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let wind_down = Addr::unchecked(&TEST_CONFIG.wind_down);

    market
        .exec_shutdown(
            &wind_down,
            ShutdownEffect::Disable,
            &[],
            &[ShutdownImpact::NewTrades],
        )
        .unwrap();

    let trader = market.clone_trader(0).unwrap();

    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap_err();

    market
        .exec_shutdown(&wind_down, ShutdownEffect::Enable, &[], &[])
        .unwrap();

    market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();
}

#[test]
fn shutdown_close_all_positions() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let wind_down = Addr::unchecked(&TEST_CONFIG.wind_down);
    let kill_switch = Addr::unchecked(&TEST_CONFIG.kill_switch);
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    market.exec_close_all_positions(&kill_switch).unwrap_err();
    market.exec_close_all_positions(&trader).unwrap_err();
    market.exec_close_all_positions(&wind_down).unwrap();

    // Since deferred execution, we will only close all positions as part of the
    // normal crank process, which means adding in one more price point after starting
    // the close of all positions.
    market.exec_refresh_price().unwrap();

    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();
    market.query_closed_position(&trader, pos_id).unwrap();
}

// let trader = market.clone_trader(0).unwrap();

// let (pos_id, _) = market
//     .exec_open_position(
//         &trader,
//         "100",
//         "10",
//         DirectionToBase::Long,
//         "1.0",
//         None,
//         None,
//         None,
//     )
//     .unwrap();

// // liquidity is adjusted due to open
// let liquidity = market.query_liquidity_stats().unwrap();

// assert_eq!(
//     liquidity,
//     LiquidityStats {
//         locked: 100u128.into(),
//         unlocked: 2_900u128.into(),
//         ..liquidity
//     }
// );

// // sanity check that the NFT works
// let nft_ids = market.query_position_token_ids(&trader).unwrap();
// assert_eq!(nft_ids.len(), 1);

// // sanity check that positions query works
// let pos = market.query_position(pos_id).unwrap();
// assert_eq!(pos.status, PositionStatus::Open);

// let positions = market.query_positions(&trader, None).unwrap();
// assert_eq!(positions.len(), 1);

// assert_eq!(positions[0].id, pos_id);
// assert_eq!(pos.id, pos_id);
