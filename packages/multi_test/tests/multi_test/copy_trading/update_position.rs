use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::{
        copy_trading::{self, WorkResp},
        market::position::PositionId,
    },
    number::{NonZero, UnsignedDecimal},
    storage::DirectionToBase,
};

use crate::copy_trading::{deposit_money, load_markets};

#[test]
fn update_position_add_collateral_impact_leverage() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    let twenty_collateral = "20".parse().unwrap();
    let collateral = NonZero::new(twenty_collateral);
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionAddCollateralImpactLeverage {
                    id: position_id,
                },
            ),
            collateral,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    let diff = initial_token
        .collateral
        .checked_sub(final_token.collateral)
        .unwrap();
    assert_eq!(diff, twenty_collateral);
}

#[test]
fn failed_update_position_add_collateral_impact_leverage() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];
    // This position doesn't exist
    let position_id = position_id.next();

    let collateral = "20".parse().unwrap();
    let collateral = NonZero::new(collateral);
    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().all(|item| item.status.finish()));

    // This will be success since queue doesn't have idea if the position id are correct
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionAddCollateralImpactLeverage {
                    id: position_id,
                },
            ),
            collateral,
        })
        .unwrap();

    // Update position. also executes the reply handler.
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();
    // No deferred work present since cranking failed
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    // Tokens are same since update failed
    assert_eq!(initial_token.collateral, final_token.collateral);

    let leader_queue = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();
    assert!(leader_queue.items.iter().any(|item| item.status.failed()));
}

#[test]
fn update_position_add_collateral_impact_size() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    let twenty_collateral = "20".parse().unwrap();
    let collateral = NonZero::new(twenty_collateral);
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionAddCollateralImpactSize {
                    id: position_id,
                    slippage_assert: None,
                },
            ),
            collateral,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    let diff = initial_token
        .collateral
        .checked_sub(final_token.collateral)
        .unwrap();
    assert_eq!(diff, twenty_collateral);
}

#[test]
fn failed_update_position_add_collateral_impact_size() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];
    // This position doesn't exist
    let position_id = position_id.next();

    let collateral = "20".parse().unwrap();
    let collateral = NonZero::new(collateral);
    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().all(|item| item.status.finish()));

    // This will be success since queue doesn't have idea if the position id are correct
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionAddCollateralImpactSize {
                    id: position_id,
                    slippage_assert: None,
                },
            ),
            collateral,
        })
        .unwrap();

    // Update position. also executes the reply handler.
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();
    // No deferred work present since cranking failed
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    // Tokens are same since update failed
    assert_eq!(initial_token.collateral, final_token.collateral);

    let leader_queue = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();
    assert!(leader_queue.items.iter().any(|item| item.status.failed()));
}

#[test]
fn update_position_remove_collateral_impact_leverage() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());
    // Set minimum_deposit_usd so that countertrade countract tries to
    // reduce the collateral instead of closing the position.
    market
        .exec_set_config(perpswap::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    let five_collateral = "5".parse().unwrap();

    let collateral = NonZero::new(five_collateral).unwrap();
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage {
                    id: position_id,
                    amount: collateral,
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Let's do deposit, to rebalance the increase of money
    market
        .exec_copytrading_mint_and_deposit(&trader, "3")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_rebalance());
    market.exec_copytrading_do_work(&trader).unwrap();
    // compute lp token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // deposit
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let estimated_crank_fee = market.query_crank_fee().unwrap();
    let estimated_final_token = initial_token
        .collateral
        .checked_add(five_collateral)
        .unwrap()
        .checked_add("3".parse().unwrap())
        .unwrap()
        .checked_sub(estimated_crank_fee)
        .unwrap();

    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    assert!(final_token.collateral.approx_eq(estimated_final_token));
    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().all(|item| item.status.finish()));
}

#[test]
fn update_position_remove_collateral_impact_leverage_failure() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    let twenty_collateral = "20".parse().unwrap();

    let collateral = NonZero::new(twenty_collateral).unwrap();
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage {
                    id: position_id,
                    amount: collateral,
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position.
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().any(|item| item.status.failed()));

    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    assert_eq!(initial_token.collateral, final_token.collateral);
}

#[test]
fn update_position_remove_collateral_impact_size() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    let five_collateral = "5".parse().unwrap();

    let collateral = NonZero::new(five_collateral).unwrap();
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                    id: position_id,
                    amount: collateral,
                    slippage_assert: None,
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Let's do deposit, to rebalance the increase of money
    market
        .exec_copytrading_mint_and_deposit(&trader, "3")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_rebalance());
    market.exec_copytrading_do_work(&trader).unwrap();
    // compute lp token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // deposit
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let estimated_final_token = initial_token
        .collateral
        .checked_add(five_collateral)
        .unwrap()
        .checked_add("3".parse().unwrap())
        .unwrap();
    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    assert!(final_token.collateral.approx_eq(estimated_final_token));
    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().all(|item| item.status.finish()));
}

#[test]
fn update_position_remove_collateral_impact_size_failure() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    let twenty_collateral = "20".parse().unwrap();

    let collateral = NonZero::new(twenty_collateral).unwrap();
    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                    id: position_id,
                    amount: collateral,
                    slippage_assert: None,
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position.
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().any(|item| item.status.failed()));

    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    assert_eq!(initial_token.collateral, final_token.collateral);
}

#[test]
fn update_position_leverage() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    // Set some crank fee configration so that crank fee logic is
    // kicked in
    market
        .exec_set_config(perpswap::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionLeverage {
                    id: position_id,
                    leverage: "6".parse().unwrap(),
                    slippage_assert: None,
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Let's do deposit, to rebalance the increase of money
    market
        .exec_copytrading_mint_and_deposit(&trader, "3")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    // compute lp token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // deposit
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let crank_fee = market.query_crank_fee().unwrap();

    let estimated_final_token = initial_token
        .collateral
        .checked_add("3".parse().unwrap())
        .unwrap()
        .checked_sub(crank_fee)
        .unwrap();
    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    assert_eq!(estimated_final_token, final_token.collateral);
    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().all(|item| item.status.finish()));
}

#[test]
fn update_position_leverage_failure() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());
    // Set some crank fee configration so that crank fee logic is
    // kicked in
    market
        .exec_set_config(perpswap::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];
    let position_id = position_id.next();

    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionLeverage {
                    id: position_id,
                    leverage: "6".parse().unwrap(),
                    slippage_assert: None,
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().any(|item| item.status.failed()));
    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    let crank_fee = market.query_crank_fee().unwrap();
    let estimated_final_collateral = initial_token.collateral.checked_sub(crank_fee).unwrap();
    assert_eq!(estimated_final_collateral, final_token.collateral);
}

#[test]
fn update_position_take_profit_leverage() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    // Set some crank fee configration so that crank fee logic is
    // kicked in
    market
        .exec_set_config(perpswap::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionTakeProfitPrice {
                    id: position_id,
                    price: "1.7".parse().unwrap(),
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Let's do deposit, to rebalance the increase of money
    market
        .exec_copytrading_mint_and_deposit(&trader, "3")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    // compute lp token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // deposit
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let crank_fee = market.query_crank_fee().unwrap();

    let estimated_final_token = initial_token
        .collateral
        .checked_add("3".parse().unwrap())
        .unwrap()
        .checked_sub(crank_fee)
        .unwrap();
    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    assert_eq!(estimated_final_token, final_token.collateral);
    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().all(|item| item.status.finish()));
}

#[test]
fn update_position_take_profit_failure() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());
    // Set some crank fee configration so that crank fee logic is
    // kicked in
    market
        .exec_set_config(perpswap::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];
    let position_id = position_id.next();

    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionTakeProfitPrice {
                    id: position_id.next(),
                    price: "1.6".parse().unwrap(),
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().any(|item| item.status.failed()));
    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    let crank_fee = market.query_crank_fee().unwrap();
    let estimated_final_collateral = initial_token.collateral.checked_sub(crank_fee).unwrap();
    assert_eq!(estimated_final_collateral, final_token.collateral);
}

#[test]
fn update_position_stop_loss() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    // Set some crank fee configration so that crank fee logic is
    // kicked in
    market
        .exec_set_config(perpswap::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];

    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionStopLossPrice {
                    id: position_id,
                    stop_loss: "0.9".parse().unwrap(),
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    // No work since cranking is not done yet
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_deferred_work());
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Let's do deposit, to rebalance the increase of money
    market
        .exec_copytrading_mint_and_deposit(&trader, "3")
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    // compute lp token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // deposit
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let crank_fee = market.query_crank_fee().unwrap();

    let estimated_final_token = initial_token
        .collateral
        .checked_add("3".parse().unwrap())
        .unwrap()
        .checked_sub(crank_fee)
        .unwrap();
    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    assert_eq!(estimated_final_token, final_token.collateral);
    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().all(|item| item.status.finish()));
}

#[test]
fn update_position_stop_loss_failure() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());
    // Set some crank fee configration so that crank fee logic is
    // kicked in
    market
        .exec_set_config(perpswap::contracts::market::config::ConfigUpdate {
            minimum_deposit_usd: Some("5".parse().unwrap()),
            crank_fee_surcharge: Some("1".parse().unwrap()),
            crank_fee_charged: Some("0.1".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    market
        .exec_copy_trading_open_position("20", DirectionToBase::Long, "1.5")
        .unwrap();
    // Open position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    // Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
    let initial_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();

    let position_id = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>()[0];
    let position_id = position_id.next();

    market
        .exec_copytrading_leader(&copy_trading::ExecuteMsg::LeaderMsg {
            market_id: market.id.clone(),
            message: Box::new(
                perpswap::storage::MarketExecuteMsg::UpdatePositionStopLossPrice {
                    id: position_id,
                    stop_loss: "0.9".parse().unwrap(),
                },
            ),
            collateral: None,
        })
        .unwrap();
    // Update position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let leader_queue = market
        .query_copy_trading_queue_status(leader.clone().into())
        .unwrap();
    assert!(leader_queue.items.iter().any(|item| item.status.failed()));
    let final_token = market.query_copy_trading_leader_tokens().unwrap().tokens[0].clone();
    let crank_fee = market.query_crank_fee().unwrap();
    let estimated_final_collateral = initial_token.collateral.checked_sub(crank_fee).unwrap();
    assert_eq!(estimated_final_collateral, final_token.collateral);
}
