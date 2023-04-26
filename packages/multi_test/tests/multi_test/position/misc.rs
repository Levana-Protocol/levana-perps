use cw2::ContractVersion;
use levana_perpswap_multi_test::time::TimeJump;
use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, response::CosmosResponseExt, PerpsApp,
};
use msg::contracts::market::entry::StatusResp;
use msg::contracts::market::{config::ConfigUpdate, position::events::PositionUpdateEvent};
use msg::prelude::*;

#[test]
// placeholder for local debug test runs
fn position_misc_debug_temp() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "10",
            "10",
            DirectionToBase::Long,
            "10",
            None,
            None,
            None,
        )
        .unwrap();

    let pos = market.query_position(pos_id).unwrap();

    let collateral_delta = Signed::<Collateral>::from_str("1.5").unwrap();
    let update_res = market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, collateral_delta)
        .unwrap();

    let pos_evt: PositionUpdateEvent = update_res
        .event_first("position-update")
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(pos_evt.active_collateral_delta, collateral_delta);

    let updated_pos = market.query_position(pos_id).unwrap();

    assert_eq!(
        updated_pos.deposit_collateral,
        pos.deposit_collateral + collateral_delta
    );
}

#[test]
fn position_misc_short_1() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_set_config(ConfigUpdate {
            minimum_deposit_usd: Some("0".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    for i in 0..100 {
        let (pos_id, _) = market
            .exec_open_position(
                &trader,
                "1",
                "5",
                DirectionToBase::Short,
                "0.5",
                None,
                None,
                None,
            )
            .unwrap();

        let _pos = market.query_position(pos_id).unwrap();

        if i % 2 == 0 {
            // close
            market.exec_close_position(&trader, pos_id, None).unwrap();

            market.query_closed_position(&trader, pos_id).unwrap();
        }
    }
}

#[test]
fn version_and_meta() {
    use msg::contracts::market::entry::QueryMsg::Version;

    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let market_version: ContractVersion = market.query(&Version {}).unwrap();
    assert_eq!(market_version.contract, "levana.finance:market");
    assert!(!market_version.version.is_empty());

    let status: StatusResp = market
        .query(&msg::contracts::market::entry::QueryMsg::Status {})
        .unwrap();
    assert!(!status.base.is_empty());
    assert!(!status.quote.is_empty());
}

#[test]
fn query_delta_neutrality_fee() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let response = market
        .query_slippage_fee("0.7".parse().unwrap(), None)
        .unwrap();
    assert!(response.fund_total.ge(&Collateral::zero()));
}

#[test]
fn position_misc_max_gains() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "82",
            "0.6",
            DirectionToBase::Short,
            "0.05",
            None,
            None,
            None,
        )
        .unwrap();

    let res = market.exec_update_position_collateral_impact_leverage(
        &trader,
        pos_id,
        "52".parse().unwrap(),
    );

    // In a collateral-is-base market, it's impossible to perform this update
    // because it will cause the direction to base to flip. In a
    // collateral-is-quote market, there is no problem because we don't have
    // off-by-one leverage.
    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            res.unwrap();
        }
        MarketType::CollateralIsBase => {
            res.unwrap_err();
        }
    }
}

#[test]
fn funding_payment_flips_direction() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    // Open a massively large long position so that the short position receives a large funding payment
    market
        .exec_open_position(
            &trader,
            "1000",
            "20",
            DirectionToBase::Long,
            "1.5",
            None,
            None,
            None,
        )
        .unwrap();

    // Now open our short position very close to flipping the leverage
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "0.01",
            DirectionToBase::Short,
            "0.001",
            None,
            None,
            None,
        )
        .unwrap();

    // Let some liquifunding occur so we collection the funding payments
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_refresh_price().unwrap();

    // In collateral-is-base markets, the position should now be in the
    // ready-to-liquidate state. For collateral-is-quote, we don't have
    // off-by-one leverage so there's no issue.
    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            market
                .query_position_pending_close(pos_id, false)
                .unwrap_err();
            market.query_position_with_pending_fees(pos_id).unwrap();
        }
        MarketType::CollateralIsBase => {
            market.query_position_pending_close(pos_id, false).unwrap();
            market.query_position_with_pending_fees(pos_id).unwrap_err();

            // If we ignore the pending fees though, it should be computable
            market.query_position(pos_id).unwrap();
        }
    }

    // And now is actually liquidated
    market.exec_crank_till_finished(&trader).unwrap();

    // Same logic as above
    match market.id.get_market_type() {
        MarketType::CollateralIsQuote => {
            market.query_position(pos_id).unwrap();
            market.query_closed_position(&trader, pos_id).unwrap_err();
        }
        MarketType::CollateralIsBase => {
            market.query_position(pos_id).unwrap_err();
            market.query_closed_position(&trader, pos_id).unwrap();
        }
    }
}
