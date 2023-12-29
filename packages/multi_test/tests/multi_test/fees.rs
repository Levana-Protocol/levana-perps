use levana_perpswap_multi_test::config::TEST_CONFIG;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::contracts::market::config::ConfigUpdate;
use msg::contracts::market::entry::Fees;
use msg::prelude::*;

fn test_fees_inner(market: PerpsMarket, direction: DirectionToBase, expected_fees: Collateral) {
    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();
    let dao_addr = Addr::unchecked(&TEST_CONFIG.dao);

    // set a specific protocol tax amount for testing
    let protocol_tax = Decimal256::from_str("0.1").unwrap();
    market
        .exec_set_config(ConfigUpdate {
            protocol_tax: Some(protocol_tax),
            trading_fee_notional_size: Some("0.001".parse().unwrap()),
            trading_fee_counter_collateral: Some("0.001".parse().unwrap()),
            delta_neutrality_fee_tax: Some(Decimal256::zero()),
            ..Default::default()
        })
        .unwrap();

    let assert_dao_eq = |expected_balance: Collateral| {
        let balance = market.query_collateral_balance(&dao_addr).unwrap();
        assert_eq!(
            balance.to_u128_with_precision(6),
            expected_balance.into_number().to_u128_with_precision(6)
        );
    };

    let assert_fees_eq = |expected_protocol, expected_lp| -> Fees {
        let fees = market.query_fees().unwrap();
        assert_eq!(fees.protocol, expected_protocol);
        assert_eq!(fees.wallets, expected_lp);

        fees
    };

    // assert initial conditions
    assert_dao_eq(Collateral::zero());
    assert_fees_eq(Collateral::zero(), Collateral::zero());

    // make sure we error when we try to transfer empty fees
    market.exec_transfer_dao_fees(&cranker).unwrap_err();
    assert_dao_eq(Collateral::zero());
    assert_fees_eq(Collateral::zero(), Collateral::zero());

    // open a position (takes some fees)
    market
        .exec_open_position(&trader, "1000", "9", direction, "1.0", None, None, None)
        .unwrap();

    let expected_protocol_fees = expected_fees.checked_mul_dec(protocol_tax).unwrap();
    let expected_lp_fees = expected_fees.checked_sub(expected_protocol_fees).unwrap();

    // assert conditions after open
    assert_dao_eq(Collateral::zero());
    let fees_after_open = market.query_fees().unwrap();
    assert_eq!(fees_after_open.protocol, expected_protocol_fees);
    assert_eq!(fees_after_open.wallets, expected_lp_fees);

    // transfer the fees (by way of factory to test submsg pipeline)
    market
        .exec_factory(&FactoryExecuteMsg::TransferAllDaoFees {})
        .unwrap();
    assert_dao_eq(expected_protocol_fees); // full original amount
    assert_fees_eq(Collateral::zero(), expected_lp_fees); // LP is unchanged

    // make sure we error when we try to transfer empty fees
    market.exec_transfer_dao_fees(&cranker).unwrap_err();
    assert_dao_eq(expected_protocol_fees); // full original amount
    assert_fees_eq(Collateral::zero(), expected_lp_fees); // LP is unchanged
}

#[test]
fn test_fees_long() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let expected_fees = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => Collateral::from_str("10"),
        MarketType::CollateralIsBase => Collateral::from_str("8.8"),
    }
    .unwrap();

    test_fees_inner(market, DirectionToBase::Long, expected_fees);
}

#[test]
fn test_fees_short() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let expected_fees = match market.id.get_market_type() {
        MarketType::CollateralIsQuote => Collateral::from_str("10"),
        MarketType::CollateralIsBase => Collateral::from_str("11.25"),
    }
    .unwrap();

    test_fees_inner(market, DirectionToBase::Short, expected_fees);
}

#[test]
fn no_fee_notional_size_reduction_perp_790() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "15",
            DirectionToBase::Long,
            "5",
            None,
            None,
            None,
        )
        .unwrap();

    let pos1 = market.query_position(pos_id).unwrap();

    println!("pos1: {:#?}", pos1);
    market
        .exec_update_position_collateral_impact_size(&trader, pos_id, "-10".parse().unwrap(), None)
        .unwrap();
    let pos2 = market.query_position(pos_id).unwrap();

    assert_eq!(pos1.trading_fee_collateral, pos2.trading_fee_collateral);
}

#[test]
fn no_fee_counter_collateral_reduction_perp_790() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let trader = market.clone_trader(0).unwrap();

    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "100",
            "15",
            DirectionToBase::Long,
            "5",
            None,
            None,
            None,
        )
        .unwrap();

    let pos1 = market.query_position(pos_id).unwrap();

    market
        .exec_update_position_max_gains(&trader, pos_id, "4".parse().unwrap())
        .unwrap();
    let pos2 = market.query_position(pos_id).unwrap();

    assert_eq!(pos1.trading_fee_collateral, pos2.trading_fee_collateral);
}
