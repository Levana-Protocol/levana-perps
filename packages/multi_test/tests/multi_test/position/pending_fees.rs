use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, time::TimeJump, PerpsApp};
use msg::{contracts::market::entry::PositionsQueryFeeApproach, prelude::*};

#[test]
fn pending_fees_in_query() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&trader, "1000000".parse().unwrap())
        .unwrap();

    // Need to open two positions so that there are funding payments to be made.
    // Make it large enough to cause delta neutrality fees to be meaningful.
    market
        .exec_open_position(
            &trader,
            "6000",
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

    let pos_orig = market
        .query_position_with_pending_fees(pos_id, PositionsQueryFeeApproach::AllFees)
        .unwrap();

    // TBD - this is currently failing, but is that really unexpected with deferred execs now?
    // i.e. some time has passed, should borrow fees not be paid while it's pending?
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

    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    market.set_time(TimeJump::Blocks(1)).unwrap();

    // Get the position information without fees so we can properly test the DNF
    // amount below.
    let pos_no_fees = market.query_position(pos_id).unwrap();
    let pos_accumulated_fees = market
        .query_position_with_pending_fees(pos_id, PositionsQueryFeeApproach::Accumulated)
        .unwrap();

    let pos = market
        .query_position_with_pending_fees(pos_id, PositionsQueryFeeApproach::AllFees)
        .unwrap();
    assert_ne!(pos.borrow_fee_collateral, Collateral::zero());
    assert_ne!(pos.borrow_fee_usd, Usd::zero());
    assert_ne!(pos.funding_fee_collateral, Signed::<Collateral>::zero());
    assert_ne!(pos.funding_fee_usd, Signed::<Usd>::zero());
    assert_ne!(pos_orig.liquidation_margin, pos.liquidation_margin);

    // We want to check that the dnf_on_close_collateral field is accurate by
    // comparing the fees before and after closing versus this value.
    let dnf_on_close = pos.dnf_on_close_collateral;

    // Actually close and make sure it matches
    market.exec_close_position(&trader, pos_id, None).unwrap();
    let closed = market.query_closed_position(&trader, pos_id).unwrap();
    assert_eq!(closed.pnl_collateral, pos.pnl_collateral);

    // Ensure that the DNF before closing (taken from pos_no_fees) plus the
    // calculated DNF amount equals the final DNF value.
    assert_eq!(
        pos_no_fees.delta_neutrality_fee_collateral + dnf_on_close,
        closed.delta_neutrality_fee_collateral
    );

    // Ensure that the PnL of the accumulated fees plus the calculated DNF equals the final closed position PnL.
    assert_eq!(
        pos.dnf_on_close_collateral,
        pos_accumulated_fees.dnf_on_close_collateral
    );
    assert_eq!(
        pos_accumulated_fees.pnl_collateral - pos_accumulated_fees.dnf_on_close_collateral,
        closed.pnl_collateral
    );
}
