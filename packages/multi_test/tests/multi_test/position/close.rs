use std::collections::HashSet;

use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, time::TimeJump, PerpsApp};
use perpswap::contracts::market::position::PositionId;
use perpswap::prelude::*;
use rand::prelude::*;

#[test]
fn position_close_works() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
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

    market.exec_close_position(&trader, pos_id, None).unwrap();

    let _pos = market.query_closed_position(&trader, pos_id).unwrap();

    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn position_close_after_close() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
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

    let _pos = market.query_position(pos_id).unwrap();
    market.exec_close_position(&trader, pos_id, None).unwrap();

    let _pos = market.query_closed_position(&trader, pos_id).unwrap();

    market
        .exec_close_position(&trader, pos_id, None)
        .unwrap_err();

    // confirm that we get no positions when we query
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn position_close_auth() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader_1 = market.clone_trader(0).unwrap();
    let trader_2 = market.clone_trader(1).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader_1,
            "100",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let _pos = market.query_position(pos_id).unwrap();

    //close for the wrong user - should get auth error
    match market.exec_close_position(&trader_2, pos_id, None) {
        Ok(_) => {
            panic!("should not have been able to close");
        }
        Err(err) => {
            let root_cause = err.root_cause().to_string();
            assert!(root_cause.contains("position owner is"));
        }
    }
    // close
    market.exec_close_position(&trader_1, pos_id, None).unwrap();

    let _pos = market.query_closed_position(&trader_1, pos_id).unwrap();

    // confirm that we get no positions when we query
    let positions = market.query_positions(&trader_1).unwrap();
    assert_eq!(positions.len(), 0);
}

#[test]
fn position_close_after_time_jump() {
    fn run(n_intervals: u64) {
        let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
        let trader = market.clone_trader(0).unwrap();
        let cranker = market.clone_trader(1).unwrap();

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

        let _pos = market.query_position(pos_id).unwrap();

        // jump forward n n_liquifunding intervals
        // println!("jumping {} epochs", n_epochs);
        market
            .set_time(TimeJump::Liquifundings(n_intervals as i64))
            .unwrap();
        market.exec_refresh_price().unwrap();
        market.exec_crank_till_finished(&cranker).unwrap();

        // close
        market.exec_close_position(&trader, pos_id, None).unwrap();

        let _pos = market.query_closed_position(&trader, pos_id).unwrap();
    }

    for i in 1..5 {
        run(i);
    }
}

#[test]
fn position_close_history() {
    struct Strategy {
        pub n_open: u32,
        pub n_close: u32,
        pub n_limit: Option<u32>,
        pub order: Option<OrderInMessage>,
    }
    impl Strategy {
        pub fn run(self) {
            let Self {
                n_open,
                n_close,
                n_limit,
                order,
            } = self;

            let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

            let trader = market.clone_trader(0).unwrap();
            let mut open_pos_ids: Vec<PositionId> = Vec::new();

            // open the positions
            for _ in 0..n_open {
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

                open_pos_ids.push(pos_id);
            }

            // shuffle the open position list
            market.with_rng(|rng| open_pos_ids.shuffle(rng));

            // close the positions
            let mut close_pos_ids = Vec::new();
            for i in 0..n_close {
                let id = open_pos_ids[i as usize];
                market.exec_close_position(&trader, id, None).unwrap();
                close_pos_ids.push(id);
            }

            let mut history_pos_ids = HashSet::new();
            let mut cursor = None;
            let mut page_number = 1;
            loop {
                let resp = market
                    .query_closed_positions(&trader, cursor.clone(), n_limit, order)
                    .unwrap();
                //println!("{:?}", resp.positions.iter().map(|pos| pos.id).collect::<Vec<PositionId>>());

                // confirm that the the page is what we expect
                // and that there are no repeats
                let mut last_close_time = None;
                for pos in &resp.positions {
                    let close_time = pos.close_time;

                    if let Some(last_close_time) = last_close_time {
                        if order.unwrap_or(OrderInMessage::Descending) == OrderInMessage::Descending
                        {
                            if close_time > last_close_time {
                                panic!(
                                    "wrong close pagination order, {} > {}",
                                    close_time, last_close_time
                                );
                            }
                        } else if close_time < last_close_time {
                            panic!(
                                "wrong close pagination order, {} < {}",
                                close_time, last_close_time
                            );
                        }
                    }

                    last_close_time = Some(close_time);

                    if history_pos_ids.contains(&pos.id) {
                        panic!("duplicate position {} in page {} while paginating close history (cursor is {:#?})", pos.id, page_number, cursor);
                    }

                    history_pos_ids.insert(pos.id);
                }

                if let Some(limit) = n_limit {
                    assert!(resp.positions.len() <= limit as usize);
                }
                cursor = resp.cursor;

                if cursor.is_none() {
                    break;
                }

                page_number += 1;
            }

            let mut history_pos_ids: Vec<PositionId> = history_pos_ids.into_iter().collect();
            close_pos_ids.sort();
            history_pos_ids.sort();
            assert_eq!(close_pos_ids, history_pos_ids);
        }
    }

    for order in [
        None,
        Some(OrderInMessage::Ascending),
        Some(OrderInMessage::Descending),
    ] {
        Strategy {
            n_open: 10,
            n_close: 10,
            n_limit: Some(5),
            order,
        }
        .run();

        Strategy {
            n_open: 10,
            n_close: 10,
            n_limit: None,
            order,
        }
        .run();

        Strategy {
            n_open: 10,
            n_close: 7,
            n_limit: Some(5),
            order,
        }
        .run();

        Strategy {
            n_open: 10,
            n_close: 3,
            n_limit: Some(5),
            order,
        }
        .run();
    }
}

#[test]
fn position_close_history_limit_1_perp_4165() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos1, _) = market
        .exec_open_position(
            &trader,
            "10",
            "5",
            DirectionToBase::Long,
            "2.5",
            None,
            None,
            None,
        )
        .unwrap();
    market.set_time(TimeJump::Blocks(5)).unwrap();
    let (pos2, _) = market
        .exec_open_position(
            &trader,
            "10",
            "5",
            DirectionToBase::Long,
            "2.5",
            None,
            None,
            None,
        )
        .unwrap();
    market.set_time(TimeJump::Blocks(5)).unwrap();
    let (pos3, _) = market
        .exec_open_position(
            &trader,
            "10",
            "5",
            DirectionToBase::Long,
            "2.5",
            None,
            None,
            None,
        )
        .unwrap();
    market.set_time(TimeJump::Blocks(5)).unwrap();

    market.exec_close_position(&trader, pos1, None).unwrap();
    market.exec_close_position(&trader, pos3, None).unwrap();
    market.exec_close_position(&trader, pos2, None).unwrap();

    let resp1 = market
        .query_closed_positions(&trader, None, Some(1), Some(OrderInMessage::Ascending))
        .unwrap();
    assert_eq!(resp1.positions[0].id, pos1);
    assert_eq!(resp1.cursor.clone().unwrap().position, pos1);
    let resp2 = market
        .query_closed_positions(
            &trader,
            resp1.cursor,
            Some(1),
            Some(OrderInMessage::Ascending),
        )
        .unwrap();
    assert_eq!(resp2.positions[0].id, pos3);
    assert_eq!(resp2.cursor.clone().unwrap().position, pos3);
    let resp3 = market
        .query_closed_positions(
            &trader,
            resp2.cursor,
            Some(1),
            Some(OrderInMessage::Ascending),
        )
        .unwrap();
    assert_eq!(resp3.positions[0].id, pos2);
    assert_eq!(resp3.cursor, None);
}
