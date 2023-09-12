use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, response::CosmosResponseExt,
    return_unless_market_collateral_quote, PerpsApp,
};
use msg::contracts::market::entry::PositionActionHistoryResp;
use msg::contracts::market::{
    entry::{LpActionKind, PositionActionKind},
    history::events::{LpActionEvent, PositionActionEvent},
};
use msg::prelude::*;
use std::ops::Neg;

#[test]
fn trade_history_works() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let summary = market.query_trade_history_summary(&trader).unwrap();
    assert_eq!(summary.trade_volume, Usd::zero());
    assert_eq!(summary.realized_pnl, Signed::zero());

    // OPEN
    let (other_pos_id, _) = market
        .exec_open_position(
            &trader,
            "90",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let (pos_id, res) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let evt: PositionActionEvent = res
        .event_first("history-position-action")
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(evt.pos_id, pos_id);
    assert_eq!(evt.action.kind, PositionActionKind::Open);
    let open_summary = market.query_trade_history_summary(&trader).unwrap();
    assert!(open_summary.trade_volume.approx_eq(1900u64.into()));
    assert_eq!(open_summary.realized_pnl, Signed::zero());

    let actions = market
        .query_position_action_history(pos_id)
        .unwrap()
        .actions;
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].kind, PositionActionKind::Open);

    // UPDATE
    let res = market
        .exec_update_position_leverage(&trader, pos_id, "20".try_into().unwrap(), None)
        .unwrap();

    let evt: PositionActionEvent = res
        .event_first("history-position-action")
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(evt.pos_id, pos_id);
    assert_eq!(evt.action.kind, PositionActionKind::Update);

    let actions = market
        .query_position_action_history(pos_id)
        .unwrap()
        .actions;
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].kind, PositionActionKind::Open);
    assert_eq!(actions[1].kind, PositionActionKind::Update);

    let actions = market
        .query_position_action_history(other_pos_id)
        .unwrap()
        .actions;
    assert_eq!(actions.len(), 1);

    let update_summary = market.query_trade_history_summary(&trader).unwrap();
    assert!(update_summary.trade_volume > open_summary.trade_volume);
    assert_eq!(update_summary.realized_pnl, Signed::zero());

    // CLOSE

    let res = market.exec_close_position(&trader, pos_id, None).unwrap();

    let evt: PositionActionEvent = res
        .event_first("history-position-action")
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(evt.pos_id, pos_id);
    assert_eq!(evt.action.kind, PositionActionKind::Close);

    let actions = market
        .query_position_action_history(pos_id)
        .unwrap()
        .actions;
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0].kind, PositionActionKind::Open);
    assert_eq!(actions[1].kind, PositionActionKind::Update);
    assert_eq!(actions[2].kind, PositionActionKind::Close);

    let actions = market
        .query_position_action_history(other_pos_id)
        .unwrap()
        .actions;
    assert_eq!(actions.len(), 1);

    let close_summary = market.query_trade_history_summary(&trader).unwrap();
    assert!(close_summary.trade_volume > update_summary.trade_volume);
    assert_ne!(close_summary.realized_pnl, Signed::zero());

    // make sure summary is really per-user
    let trader = market.clone_trader(1).unwrap();
    let summary = market.query_trade_history_summary(&trader).unwrap();
    assert_eq!(summary.trade_volume, Usd::zero());
    assert_eq!(summary.realized_pnl, Signed::zero());
}

#[test]
fn lp_history_works() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let lp = Addr::unchecked("new_lp");

    let summary = market.query_lp_info(&lp).unwrap().history;
    assert_eq!(summary.deposit_usd, Usd::zero());
    assert_eq!(summary.yield_usd, Usd::zero());

    let actions = market.query_lp_action_history(&lp).unwrap().actions;
    assert_eq!(actions.len(), 0);

    let deposit = Number::from(100u64);

    // DEPOSIT
    let res = market
        .exec_mint_and_deposit_liquidity(&lp, deposit)
        .unwrap();

    let summary = market.query_lp_info(&lp).unwrap().history;
    assert_eq!(summary.deposit_usd.into_number(), deposit);
    assert_eq!(summary.yield_usd, Usd::zero());
    let evt: LpActionEvent = res
        .event_first("history-lp-action")
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(evt.addr, lp);
    assert_eq!(evt.action.kind, LpActionKind::DepositLp);
    let actions = market.query_lp_action_history(&lp).unwrap().actions;
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].kind, LpActionKind::DepositLp);

    // WITHDRAW
    let res = market.exec_withdraw_liquidity(&lp, None).unwrap();

    let summary = market.query_lp_info(&lp).unwrap().history;
    // in terms of historical summary, it's still 100
    assert_eq!(summary.deposit_usd.into_number(), deposit);
    assert_eq!(summary.yield_usd, Usd::zero());
    let evt: LpActionEvent = res
        .event_first("history-lp-action")
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(evt.addr, lp);
    assert_eq!(evt.action.kind, LpActionKind::Withdraw);
    let actions = market.query_lp_action_history(&lp).unwrap().actions;

    // println!("{:#?}", actions);
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].kind, LpActionKind::DepositLp);
    assert_eq!(actions[1].kind, LpActionKind::Withdraw);

    // TODO - more history tests for XLP, unstake, claim yield, etc.
}

#[test]
fn trade_volume_precise() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    market.exec_set_price("7".parse().unwrap()).unwrap();

    let summary = market.query_trade_history_summary(&trader).unwrap();
    assert_eq!(summary.trade_volume, Usd::zero());
    assert_eq!(summary.realized_pnl, Signed::zero());

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "90",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let summary = market.query_trade_history_summary(&trader).unwrap();
    let market_type = market.id.get_market_type();

    match market_type {
        MarketType::CollateralIsQuote => {
            assert!(
                summary.trade_volume.approx_eq("900".parse().unwrap()),
                "trade_volume {} does not approximately equal expected 900",
                summary.trade_volume,
            );
            assert_eq!(summary.realized_pnl, Signed::zero());
        }
        MarketType::CollateralIsBase => {
            assert!(
                summary.trade_volume.approx_eq("6300".parse().unwrap()),
                "trade_volume {} does not approximately equal expected 6930",
                summary.trade_volume,
            );
            assert_eq!(summary.realized_pnl, Signed::zero());
        }
    }

    market
        .exec_update_position_collateral_impact_size(&trader, pos_id, "100".parse().unwrap(), None)
        .unwrap();
    assert!(
        market
            .query_trade_history_summary(&trader)
            .unwrap()
            .trade_volume
            > summary.trade_volume
    );

    market.exec_close_position(&trader, pos_id, None).unwrap();
    assert!(
        market
            .query_trade_history_summary(&trader)
            .unwrap()
            .trade_volume
            > summary.trade_volume
    );
}

#[test]
fn trade_history_update_fee_792() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_quote!(market);
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    // no fee when updating with "impacts leverage"
    let res = market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "20".try_into().unwrap())
        .unwrap();

    let evt: PositionActionEvent = res
        .event_first("history-position-action")
        .unwrap()
        .try_into()
        .unwrap();

    assert!(evt.action.trade_fee.is_none());
    assert_eq!(evt.action.kind, PositionActionKind::Update);

    // yes fee when updating with "impacts size"
    let res = market
        .exec_update_position_collateral_impact_size(
            &trader,
            pos_id,
            "20".try_into().unwrap(),
            None,
        )
        .unwrap();

    let evt: PositionActionEvent = res
        .event_first("history-position-action")
        .unwrap()
        .try_into()
        .unwrap();

    assert!(evt.action.trade_fee.unwrap() > Usd::zero());
    assert_eq!(evt.action.kind, PositionActionKind::Update);
}

#[test]
fn trade_history_nft_transfer_perp_963() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();
    let new_owner = market.clone_trader(1).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let res = market
        .exec_position_token_transfer(&pos_id.to_string(), &trader, &new_owner)
        .unwrap();

    let evt: PositionActionEvent = res
        .event_first("history-position-action")
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(evt.action.trade_fee, None);
    assert_eq!(evt.action.kind, PositionActionKind::Transfer);
    assert_eq!(evt.action.old_owner, Some(trader.clone()));
    assert_eq!(evt.action.new_owner, Some(new_owner.clone()));

    let PositionActionHistoryResp {
        mut actions,
        next_start_after,
    } = market.query_position_action_history(pos_id).unwrap();
    assert_eq!(next_start_after, None);

    assert_eq!(actions.len(), 2);

    let transfer = actions.pop().unwrap();
    let open = actions.pop().unwrap();

    assert_eq!(transfer.kind, PositionActionKind::Transfer);
    assert_eq!(transfer.old_owner, Some(trader.clone()));
    assert_eq!(transfer.new_owner, Some(new_owner.clone()));
    assert_eq!(transfer.trade_fee, None);

    assert_eq!(open.kind, PositionActionKind::Open);
    assert_eq!(open.old_owner, None);
    assert_eq!(open.new_owner, None);

    assert_eq!(open.collateral, transfer.collateral);

    let old_owner_actions = market.query_trader_action_history(&trader).unwrap();
    let mut old_owner_transfer = transfer.clone();
    old_owner_transfer.transfer_collateral = old_owner_transfer.transfer_collateral.neg();
    assert_eq!(&old_owner_actions.actions, &[open, old_owner_transfer]);
    assert_eq!(old_owner_actions.next_start_after, None);

    let new_owner_actions = market.query_trader_action_history(&new_owner).unwrap();
    assert_eq!(&new_owner_actions.actions, &[transfer]);
    assert_eq!(new_owner_actions.next_start_after, None);
}

#[test]
fn price_history_works() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let start_prices = market
        .query_spot_price_history(None, None, None)
        .unwrap();

    // there is an initial price due to liquidity deposit, and initial market setup
    let initial_price_len = 2;

    // sanity check to confirm 
    assert_eq!(start_prices.len(), initial_price_len);

    for i in initial_price_len..10 {
        market
            .exec_set_price(format!("{}", i).parse().unwrap())
            .unwrap();
    }

    let prices = market
        .query_spot_price_history(None, None, Some(OrderInMessage::Ascending))
        .unwrap();

    assert_eq!(prices.len(), 10);

    // check just the new prices we added 
    for (i, price) in prices.iter().skip(initial_price_len).enumerate() {
        assert_eq!(price.price_base, (i+initial_price_len).to_string().parse().unwrap());
    }

    let prices = market
        .query_spot_price_history(None, None, Some(OrderInMessage::Descending))
        .unwrap();

    assert_eq!(prices.len(), 10);

    // check just the new prices we added 
    for (i, price) in prices.iter().rev().skip(initial_price_len).rev().enumerate() {
        assert_eq!(price.price_base, (9 - i).to_string().parse().unwrap());
    }

    let prices_desc_page_1 = market
        .query_spot_price_history(None, Some(3), Some(OrderInMessage::Descending))
        .unwrap();
    let prices_desc_page_2 = market
        .query_spot_price_history(
            Some(prices_desc_page_1.last().unwrap().timestamp),
            None,
            Some(OrderInMessage::Descending),
        )
        .unwrap();
    let prices_desc = prices_desc_page_1
        .iter()
        .chain(prices_desc_page_2.iter())
        .map(|p| p.price_base.to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        vec!["9", "8", "7", "6", "5", "4", "3", "2", "1"],
        prices_desc.iter().rev().skip(initial_price_len - 1).rev().collect::<Vec<_>>()
    );

    let prices_asc_page_1 = market
        .query_spot_price_history(None, Some(3), Some(OrderInMessage::Ascending))
        .unwrap();
    let prices_asc_page_2 = market
        .query_spot_price_history(
            Some(prices_asc_page_1.last().unwrap().timestamp),
            None,
            Some(OrderInMessage::Ascending),
        )
        .unwrap();
    let prices_asc = prices_asc_page_1
        .iter()
        .chain(prices_asc_page_2.iter())
        .map(|p| p.price_base.to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        vec!["1", "2", "3", "4", "5", "6", "7", "8", "9"],
        prices_asc.iter().skip(initial_price_len - 1).collect::<Vec<_>>()
    );
}

#[test]
fn lp_history_works_bidirectional() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let lp = Addr::unchecked("new_lp");

    let summary = market.query_lp_info(&lp).unwrap().history;
    assert_eq!(summary.deposit_usd, Usd::zero());
    assert_eq!(summary.yield_usd, Usd::zero());

    let actions = market.query_lp_action_history(&lp).unwrap().actions;
    assert_eq!(actions.len(), 0);

    // DEPOSIT
    for i in 1..100 {
        market
            .exec_mint_and_deposit_liquidity(&lp, i.to_string().parse().unwrap())
            .unwrap();
    }

    let asc = market
        .query_lp_action_history_full(&lp, OrderInMessage::Ascending)
        .unwrap();
    let mut desc = market
        .query_lp_action_history_full(&lp, OrderInMessage::Descending)
        .unwrap();
    desc.reverse();
    assert_eq!(asc, desc);
}
