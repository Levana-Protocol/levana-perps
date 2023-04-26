use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, time::TimeJump, PerpsApp};
use msg::prelude::*;

#[test]
fn pending_fees_in_query() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.automatic_time_jump_enabled = false;
    let trader = market.clone_trader(0).unwrap();

    // Need to open two positions so that there are funding payments to be made.
    market
        .exec_open_position(
            &trader,
            "6",
            "15",
            DirectionToBase::Long,
            "3",
            None,
            None,
            None,
        )
        .unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "10",
            "15",
            DirectionToBase::Short,
            "3",
            None,
            None,
            None,
        )
        .unwrap();

    let pos_orig = market.query_position_with_pending_fees(pos_id).unwrap();
    assert_eq!(pos_orig.borrow_fee_collateral, Collateral::zero());
    assert_eq!(pos_orig.borrow_fee_usd, Usd::zero());
    assert_eq!(
        pos_orig.funding_fee_collateral,
        Signed::<Collateral>::zero()
    );
    assert_eq!(pos_orig.funding_fee_usd, Signed::<Usd>::zero());

    market.set_time(TimeJump::Hours(2)).unwrap();

    let pos_no_pending_fees = market.query_position(pos_id).unwrap();
    assert_eq!(
        pos_no_pending_fees.borrow_fee_collateral,
        Collateral::zero()
    );
    assert_eq!(pos_no_pending_fees.borrow_fee_usd, Usd::zero());
    assert_eq!(
        pos_no_pending_fees.funding_fee_collateral,
        Signed::<Collateral>::zero()
    );
    assert_eq!(pos_no_pending_fees.funding_fee_usd, Signed::<Usd>::zero());
    assert_eq!(
        pos_orig.liquidation_margin,
        pos_no_pending_fees.liquidation_margin
    );

    let pos = market.query_position_with_pending_fees(pos_id).unwrap();
    assert_ne!(pos.borrow_fee_collateral, Collateral::zero());
    assert_ne!(pos.borrow_fee_usd, Usd::zero());
    assert_ne!(pos.funding_fee_collateral, Signed::<Collateral>::zero());
    assert_ne!(pos.funding_fee_usd, Signed::<Usd>::zero());
    assert_ne!(pos_orig.liquidation_margin, pos.liquidation_margin);
}
