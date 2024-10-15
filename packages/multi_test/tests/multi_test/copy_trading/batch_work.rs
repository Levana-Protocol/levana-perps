use cosmwasm_std::{Addr, Event};
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::{
        copy_trading::{self, FactoryConfigUpdate, WorkResp},
        market::position::PositionId,
    },
    number::Collateral,
    storage::DirectionToBase,
};

use crate::copy_trading::{deposit_money, load_markets};

#[test]
fn rebalance_pagination() {
    // This test invokes the pagination API from the closed positions.
    let perps_app = PerpsApp::new_cell().unwrap();
    let factory = perps_app.borrow_mut().factory_addr.clone();
    let market = PerpsMarket::new(perps_app).unwrap();
    // Have a low limit to allow batching
    market
        .exec_copytrading(
            &factory,
            &copy_trading::ExecuteMsg::FactoryUpdateConfig(FactoryConfigUpdate {
                allowed_rebalance_queries: Some(2),
                allowed_lp_token_queries: None,
            }),
        )
        .unwrap();

    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "20000").unwrap();

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "20000".parse().unwrap());

    for _ in 1..20 {
        // Leader opens a position
        market
            .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
            .unwrap();
        // Process queue item: Open the position
        market.exec_copytrading_do_work(&trader).unwrap();
        market.exec_crank_till_finished(&lp).unwrap();

        // Process queue item: Handle deferred exec id
        market.exec_copytrading_do_work(&trader).unwrap();
    }

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // We are going to make a profit!
    market.exec_set_price("1.5".try_into().unwrap()).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let all_position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(all_position_ids.len(), 0);

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let trader1 = market.clone_trader(1).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader1, "20")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    match work {
        WorkResp::NoWork => panic!("Impossible: No work"),
        WorkResp::HasWork { work_description } => assert!(work_description.is_rebalance()),
    }

    // Rebalance the market
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-rebalanced")
        .add_attribute("made-profit", true.to_string())
        .add_attribute("batched", false.to_string());
    response.assert_event(&event);

    // Compute lp token value
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let token_event = response
        .events
        .iter()
        .find(|item| item.ty == "wasm-lp-token")
        .unwrap();
    let token_value = token_event
        .attributes
        .iter()
        .find(|item| item.key == "value")
        .unwrap();
    // Token value has increased
    assert!(Collateral::one() < token_value.value.parse().unwrap());
    // Do deposit
    market.exec_copytrading_do_work(&trader1).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let tokens = market.query_copy_trading_balance(&trader1).unwrap();
    let shares = tokens.balance[0].shares;
    // Since token value has increased, he can buy less shares for the same amount
    assert!(shares.raw() < "20".parse().unwrap());
}

#[test]
fn batch_work_rebalance() {
    // This test invokes the pagination API from the closed positions.
    let perps_app = PerpsApp::new_cell().unwrap();
    let factory = perps_app.borrow_mut().factory_addr.clone();
    let market = PerpsMarket::new(perps_app).unwrap();
    // Have a low limit to allow batching
    market
        .exec_copytrading(
            &factory,
            &copy_trading::ExecuteMsg::FactoryUpdateConfig(FactoryConfigUpdate {
                allowed_rebalance_queries: Some(2),
                allowed_lp_token_queries: None,
            }),
        )
        .unwrap();

    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "20000").unwrap();

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "20000".parse().unwrap());

    for _ in 1..32 {
        // Leader opens a position
        market
            .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
            .unwrap();
        // Process queue item: Open the position
        market.exec_copytrading_do_work(&trader).unwrap();
        market.exec_crank_till_finished(&lp).unwrap();

        // Process queue item: Handle deferred exec id
        market.exec_copytrading_do_work(&trader).unwrap();
    }

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // We are going to make a profit!
    market.exec_set_price("1.5".try_into().unwrap()).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let all_position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap()
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(all_position_ids.len(), 0);

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let trader1 = market.clone_trader(1).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader1, "20")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    match work {
        WorkResp::NoWork => panic!("Impossible: No work"),
        WorkResp::HasWork { work_description } => assert!(work_description.is_rebalance()),
    }

    // Rebalance the market
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-rebalanced")
        .add_attribute("made-profit", true.to_string())
        .add_attribute("batched", true.to_string());
    response.assert_event(&event);

    let work = market.query_copy_trading_work().unwrap();
    match work {
        WorkResp::NoWork => panic!("Impossible: No work"),
        WorkResp::HasWork { work_description } => assert!(work_description.is_rebalance()),
    }

    // Rebalance the market
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-rebalanced")
        .add_attribute("made-profit", true.to_string())
        .add_attribute("batched", false.to_string());
    response.assert_event(&event);

    // Compute lp token value
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let token_event = response
        .events
        .iter()
        .find(|item| item.ty == "wasm-lp-token")
        .unwrap();
    let token_value = token_event
        .attributes
        .iter()
        .find(|item| item.key == "value")
        .unwrap();
    // Token value has increased
    assert!(Collateral::one() < token_value.value.parse().unwrap());
    // Do deposit
    market.exec_copytrading_do_work(&trader1).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let tokens = market.query_copy_trading_balance(&trader1).unwrap();
    let shares = tokens.balance[0].shares;
    // Since token value has increased, he can buy less shares for the same amount
    assert!(shares.raw() < "20".parse().unwrap());
}

#[test]
fn no_deferred_work_lost_for_open_position() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);
    deposit_money(&market, &trader, "2000").unwrap();

    let mut open_positions = 0;
    for _ in 0..=2 {
        // Leader opens a position
        open_positions += 1;
        market
            .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
            .unwrap();
        market.exec_crank_till_finished(&trader).unwrap();
    }

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();
    assert_eq!(items.items.len(), 3);

    let mut deferred_works = 0;
    loop {
        market.exec_crank_till_finished(&trader).unwrap();
        let work = market.query_copy_trading_work().unwrap();
        if work.has_work() {
            if work.is_deferred_work() {
                deferred_works += 1;
            }
            market.exec_copytrading_do_work(&trader).unwrap();
        } else {
            break;
        }
    }
    assert_eq!(open_positions, deferred_works);
}

#[test]
fn batch_work_lp_token_only_positions() {
    let perps_app = PerpsApp::new_cell().unwrap();
    let factory = perps_app.borrow_mut().factory_addr.clone();
    let market = PerpsMarket::new(perps_app).unwrap();
    // Have a low limit to allow batching
    market
        .exec_copytrading(
            &factory,
            &copy_trading::ExecuteMsg::FactoryUpdateConfig(FactoryConfigUpdate {
                allowed_rebalance_queries: None,
                allowed_lp_token_queries: Some(3),
            }),
        )
        .unwrap();

    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();

    load_markets(&market);
    deposit_money(&market, &trader, "20000").unwrap();

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "20000".parse().unwrap());

    for _ in 1..32 {
        // Leader opens a position
        market
            .exec_copy_trading_open_position("10", DirectionToBase::Long, "1.5")
            .unwrap();
        // Process queue item: Open the position
        market.exec_copytrading_do_work(&trader).unwrap();
        market.exec_crank_till_finished(&lp).unwrap();

        // Process queue item: Handle deferred exec id
        market.exec_copytrading_do_work(&trader).unwrap();
    }

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let trader1 = market.clone_trader(1).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader1, "20")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    // Compute lp token value
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-lp-token")
        .add_attribute("batched", true.to_string())
        .add_attribute("open-positions", "20".to_string())
        .add_attribute("open-orders", "0".to_string());
    response.assert_event(&event);

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-lp-token")
        .add_attribute("batched", true.to_string())
        .add_attribute("open-positions", "31".to_string())
        .add_attribute("open-orders", "0".to_string());
    response.assert_event(&event);

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-lp-token")
        .add_attribute("batched", true.to_string())
        .add_attribute("validated-open-positions", "20".to_string());
    response.assert_event(&event);

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-lp-token")
        .add_attribute("batched", true.to_string())
        .add_attribute("validated-open-positions", "31".to_string());
    response.assert_event(&event);

    let work = market.query_copy_trading_work().unwrap();
    assert!(work.is_compute_lp_token());
    let response = market.exec_copytrading_do_work(&trader1).unwrap();
    let event = Event::new("wasm-lp-token")
        .add_attribute("batched", false.to_string());
    response.assert_event(&event);

    let token_event = response
        .events
        .iter()
        .find(|item| item.ty == "wasm-lp-token")
        .unwrap();
    let token_value = token_event
        .attributes
        .iter()
        .find(|item| item.key == "value")
        .unwrap();
    // Token value has decrease since we only opened positions
    assert!(Collateral::one() > token_value.value.parse().unwrap());
    // Do deposit
    market.exec_copytrading_do_work(&trader1).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let tokens = market.query_copy_trading_balance(&trader1).unwrap();
    let shares = tokens.balance[0].shares;
    // Since token value has decreased, he can buy more shares for the same amount
    assert!(shares.raw() > "20".parse().unwrap());
}
