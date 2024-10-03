use std::str::FromStr;

use cosmwasm_std::{Addr, Event};
use levana_perpswap_multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};
use msg::{
    contracts::{
        copy_trading::{
            DecQueuePositionId, IncQueueItem, IncQueuePositionId, ProcessingStatus, QueueItem,
            QueueItemStatus, QueuePositionId, WorkDescription, WorkResp,
        },
        market::position::PositionId,
    },
    shared::number::{Collateral, NonZero},
};

#[test]
fn query_config() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market.query_copy_trading_config().unwrap();
}

#[test]
fn deposit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let token = market.get_copytrading_token().unwrap();

    load_markets(&market);

    let trader = market.clone_trader(0).unwrap();

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    let response = market
        .query_copy_trading_queue_status(trader.into())
        .unwrap();
    assert_eq!(response.items.len(), 1);
    let item = &response.items[0];

    assert_eq!(
        item,
        &QueueItemStatus {
            item: QueueItem::IncCollaleteral {
                item: IncQueueItem::Deposit {
                    funds: NonZero::new(Collateral::from_str("100").unwrap()).unwrap(),
                    token,
                },
                id: IncQueuePositionId::new(0)
            },
            status: ProcessingStatus::NotProcessed
        }
    );
    assert!(response.inc_processed_till.is_none())
}

fn load_markets(market: &PerpsMarket) {
    let trader = market.clone_trader(0).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::LoadMarket {}
        }
    );

    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::LoadMarket {}
        }
    );
    market.exec_copytrading_do_work(&trader).unwrap();
}

#[test]
fn initial_no_work() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    load_markets(&market);

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork)
}

#[test]
fn detect_process_queue_item_work() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let token = market.get_copytrading_token().unwrap();

    load_markets(&market);

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    // Needs to compute lp token value for the initial deposit
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ComputeLpTokenValue {
                token
            }
        }
    );
    let resp = market.exec_copytrading_do_work(&trader).unwrap();
    assert!(resp.has_event(
        &Event::new("wasm-lp-token").add_attribute("value", Collateral::one().to_string())
    ));

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ProcessQueueItem {
                id: QueuePositionId::IncQueuePositionId(IncQueuePositionId::new(0))
            }
        }
    );
}

fn deposit_money(market: &PerpsMarket, trader: &Addr, amount: &str) {
    market
        .exec_copytrading_mint_and_deposit(trader, amount)
        .unwrap();
    let token = market.get_copytrading_token().unwrap();

    let work = market.query_copy_trading_work().unwrap();
    // Needs to compute lp token value for the initial deposit
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ComputeLpTokenValue {
                token
            }
        }
    );

    // Compute LP token value
    market.exec_copytrading_do_work(trader).unwrap();
    // Process queue item: do the actual deposit
    market.exec_copytrading_do_work(trader).unwrap();

    // Should not find any work now
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
}

#[test]
fn do_actual_deposit() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Compute LP token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // Process queue item: do the actual deposit
    let resp = market.exec_copytrading_do_work(&trader).unwrap();
    assert!(resp.has_event(
        &Event::new("wasm-deposit")
            .add_attribute("funds", "100".to_owned())
            .add_attribute("shares", "100".to_owned())
    ));

    // Should not find any work now
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let balance = market.query_copy_trading_balance(&trader).unwrap();
    assert_eq!(balance.balance.len(), 1);
    assert_eq!(balance.balance[0].shares, "100".parse().unwrap());
    let token = market.get_copytrading_token().unwrap();
    assert_eq!(balance.balance[0].token, token);

    let another_trader = market.clone_trader(1).unwrap();
    let balance = market.query_copy_trading_balance(&another_trader).unwrap();
    assert!(balance.balance.is_empty());

    let queue_resp = market
        .query_copy_trading_queue_status(trader.into())
        .unwrap();
    assert_eq!(queue_resp.items.len(), 1);
    assert_eq!(queue_resp.items[0].status, ProcessingStatus::Finished);
}

#[test]
fn does_not_compute_lp_token_work() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Compute initial LP token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // Process queue item: do the actual deposit
    market.exec_copytrading_do_work(&trader).unwrap();

    // Should not find any work now
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // Now let's do another deposit, so that it has to compute lp token value
    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Should not compute LP token value since there has been no positions opened etc.
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ProcessQueueItem {
                id: QueuePositionId::IncQueuePositionId(IncQueuePositionId::new(1))
            }
        }
    );
    // Process queue item: Do actual deposit
    market.exec_copytrading_do_work(&trader).unwrap();
}

#[test]
fn do_withdraw() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Compute LP token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // Process queue item: do the actual deposit
    market.exec_copytrading_do_work(&trader).unwrap();

    let initial_balance = market.query_copy_trading_balance(&trader).unwrap();
    assert_eq!(initial_balance.balance[0].shares, "100".parse().unwrap());

    market
        .exec_copytrading_withdrawal(&trader, "101")
        .unwrap_err();
    market.exec_copytrading_withdrawal(&trader, "50").unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: msg::contracts::copy_trading::WorkDescription::ProcessQueueItem {
                id: QueuePositionId::DecQueuePositionId(DecQueuePositionId::new(0))
            }
        }
    );
    // Process queue item: do the actual withdrawal
    market.exec_copytrading_do_work(&trader).unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let new_balance = market.query_copy_trading_balance(&trader).unwrap();
    assert_eq!(new_balance.balance[0].shares, "50".parse().unwrap());

    market
        .exec_copytrading_withdrawal(&trader, "51")
        .unwrap_err();
    market.exec_copytrading_withdrawal(&trader, "50").unwrap();
    // Process queue item: do the actual withdrawal
    market.exec_copytrading_do_work(&trader).unwrap();
    market
        .exec_copytrading_withdrawal(&trader, "1")
        .unwrap_err();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let queue_resp = market
        .query_copy_trading_queue_status(trader.into())
        .unwrap();
    assert_eq!(queue_resp.items.len(), 3);
    assert!(queue_resp
        .items
        .iter()
        .all(|item| item.status == ProcessingStatus::Finished));
}

#[test]
fn load_market_after_six_hours() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    load_markets(&market);

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let six_hours = 6 * 60 * 60;
    market
        .set_time(levana_perpswap_multi_test::time::TimeJump::Seconds(
            six_hours - 1,
        ))
        .unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    market
        .set_time(levana_perpswap_multi_test::time::TimeJump::Seconds(2))
        .unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::LoadMarket {}
        }
    );
}

#[test]
fn query_leader_tokens() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Compute LP token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // Process queue item: do the actual deposit
    market.exec_copytrading_do_work(&trader).unwrap();

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens.len(), 1);

    assert_eq!(tokens[0].collateral, "100".parse().unwrap());
    assert_eq!(tokens[0].shares, "100".parse().unwrap());

    market.exec_copytrading_withdrawal(&trader, "50").unwrap();
    market.exec_copytrading_do_work(&trader).unwrap();

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens.len(), 1);

    assert_eq!(tokens[0].collateral, "50".parse().unwrap());
    assert_eq!(tokens[0].shares, "100".parse().unwrap());
}

#[test]
fn leader_opens_correct_position() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);

    deposit_money(&market, &trader, "200");
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader queues to open a new position
    market.exec_ct_leader("50").unwrap();
    // market.exec_ct_leader("2.5").unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::ProcessQueueItem {
                id: QueuePositionId::DecQueuePositionId(DecQueuePositionId::new(0))
            }
        }
    );

    // Process queue item: Open the position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap();
    let position_ids = position_ids
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();
    assert!(position_ids.len() == 1);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "150".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();
    // We don't know if we were able to successfully finish
    assert!(items.items.iter().any(|item| item.status.pending()));

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::HandleDeferredExecId {}
        }
    );

    // Process queue item: Handle deferred exec id
    market.exec_copytrading_do_work(&trader).unwrap();

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    // No change in available collateral
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "150".parse().unwrap());
}

#[test]
fn withdraw_bug_perp_4159() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    load_markets(&market);

    market
        .exec_copytrading_mint_and_deposit(&trader, "100")
        .unwrap();

    // Compute LP token value
    market.exec_copytrading_do_work(&trader).unwrap();
    // Process queue item: do the actual deposit
    market.exec_copytrading_do_work(&trader).unwrap();

    // Issue full withdrawal
    market.exec_copytrading_withdrawal(&trader, "100").unwrap();
    // You can still issue full withdrawal since withdrawal action has not been executed yet
    market.exec_copytrading_withdrawal(&trader, "100").unwrap();

    // Does full withdrawal
    market.exec_copytrading_do_work(&trader).unwrap();
    // This should not fail making the queue stuck!
    market.exec_copytrading_do_work(&trader).unwrap();
    // There should be no work now
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);

    let items = market
        .query_copy_trading_queue_status(trader.into())
        .unwrap();
    // The invalid withdrawal failed
    assert!(items.items.iter().any(|item| item.status.failed()))
}

#[test]
fn leader_opens_attempt_open_incorrect_position() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let lp = market.clone_lp(0).unwrap();
    let leader = Addr::unchecked(TEST_CONFIG.protocol_owner.clone());

    load_markets(&market);

    deposit_money(&market, &trader, "200");
    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    // Leader queues to open a position that will fail eventually
    market.exec_ct_leader("2.5").unwrap();
    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(
        work,
        WorkResp::HasWork {
            work_description: WorkDescription::ProcessQueueItem {
                id: QueuePositionId::DecQueuePositionId(DecQueuePositionId::new(0))
            }
        }
    );

    // Process queue item: Open the position
    market.exec_copytrading_do_work(&trader).unwrap();
    market.exec_crank_till_finished(&lp).unwrap();

    let position_ids = market
        .query_position_token_ids(&market.copy_trading_addr)
        .unwrap();
    let position_ids = position_ids
        .iter()
        .map(|item| PositionId::new(item.parse().unwrap()))
        .collect::<Vec<_>>();
    assert!(position_ids.len() == 0);

    let status = market.query_copy_trading_leader_tokens().unwrap();
    let tokens = status.tokens;
    // Available collateral hasn't changed
    assert_eq!(tokens[0].collateral, "200".parse().unwrap());

    let items = market
        .query_copy_trading_queue_status(leader.into())
        .unwrap();
    assert_eq!(items.items.len(), 1);
    // All the items should have been finished
    assert!(!items.items.iter().all(|item| item.status.finish()));

    let work = market.query_copy_trading_work().unwrap();
    assert_eq!(work, WorkResp::NoWork);
}
