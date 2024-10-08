use cosmwasm_std::{Addr, Decimal256};
use perpswap::contracts::market::position::{
    LiquidationMargin, Position, PositionId, SignedCollateralAndUsd,
};
use perpswap::prelude::*;

#[test]
fn position_data_long() {
    let price = Price::try_from_number(Number::from_ratio_u256(100u64, 1u64)).unwrap();
    let price_usd =
        PriceCollateralInUsd::try_from_number(Number::from_ratio_u256(100u64, 1u64)).unwrap();
    let market_type = MarketType::CollateralIsQuote;
    let price_point = PricePoint {
        price_notional: price,
        price_usd,
        timestamp: Default::default(),
        price_base: price.into_base_price(market_type),
        is_notional_usd: true,
        market_type,
        publish_time: None,
        publish_time_usd: None,
    };

    let pos = Position {
        owner: Addr::unchecked("trader".to_string()),
        id: PositionId::new(123),
        deposit_collateral: SignedCollateralAndUsd::new("10".parse().unwrap(), &price_point),
        active_collateral: "10".parse().unwrap(),
        counter_collateral: "25".parse().unwrap(),
        notional_size: "1".parse().unwrap(),
        //direction: Direction::Long,
        created_at: Default::default(),
        price_point_created_at: Default::default(),
        trading_fee: Default::default(),
        funding_fee: Default::default(),
        borrow_fee: Default::default(),
        crank_fee: Default::default(),
        delta_neutrality_fee: Default::default(),
        liquifunded_at: Default::default(),
        next_liquifunding: Default::default(),
        stop_loss_override: None,
        take_profit_trader: None,
        liquidation_margin: Default::default(),
        liquidation_price: None,
        take_profit_total: None,
        stop_loss_override_notional: None,
        take_profit_trader_notional: None,
    };

    let liquidation_price =
        pos.liquidation_price(price, pos.active_collateral, &LiquidationMargin::default());

    // 3x long collateral -2x short notional
    assert_eq!(
        pos.active_leverage_to_notional(&price_point)
            .into_base(market_type)
            .unwrap()
            .split()
            .1
            .raw(),
        "10".parse::<Decimal256>().unwrap()
    );

    // +50% to liquidation; -25% to take profit
    assert_eq!(
        liquidation_price.unwrap().into_number(),
        Number::from(90u64)
    );
    assert_eq!(
        pos.take_profit_price_total(&price_point, MarketType::CollateralIsQuote)
            .unwrap()
            .unwrap()
            .into_number(),
        Number::from(125u64)
    );
}

#[test]
fn position_data_short() {
    let price = Price::try_from_number(Number::from_ratio_u256(1u64, 10u64)).unwrap();
    let price_usd =
        PriceCollateralInUsd::try_from_number(Number::from_ratio_u256(1u64, 10u64)).unwrap();
    let market_type = MarketType::CollateralIsBase;
    let price_point = PricePoint {
        price_notional: price,
        price_usd,
        timestamp: Default::default(),
        price_base: price.into_base_price(market_type),
        is_notional_usd: true,
        market_type,
        publish_time: None,
        publish_time_usd: None,
    };

    let pos = Position {
        owner: Addr::unchecked("trader".to_string()),
        id: PositionId::new(123),
        deposit_collateral: SignedCollateralAndUsd::new("1000".parse().unwrap(), &price_point),
        active_collateral: "1000".parse().unwrap(),
        counter_collateral: "500".parse().unwrap(),
        notional_size: "-20000".parse().unwrap(),
        created_at: Default::default(),
        price_point_created_at: Default::default(),
        trading_fee: Default::default(),
        funding_fee: Default::default(),
        borrow_fee: Default::default(),
        crank_fee: Default::default(),
        delta_neutrality_fee: Default::default(),
        liquifunded_at: Default::default(),
        next_liquifunding: Default::default(),
        stop_loss_override: None,
        take_profit_trader: None,
        liquidation_margin: Default::default(),
        liquidation_price: None,
        take_profit_total: None,
        stop_loss_override_notional: None,
        take_profit_trader_notional: None,
    };

    let liquidation_price =
        pos.liquidation_price(price, pos.active_collateral, &LiquidationMargin::default());

    // 3x long collateral -2x short notional
    assert_eq!(
        pos.active_leverage_to_notional(&price_point)
            .into_base(market_type)
            .unwrap()
            .split()
            .1
            .into_number(),
        Number::from(3u64)
    );

    // +50% to liquidation; -25% to take profit
    assert_eq!(
        liquidation_price.unwrap().into_number(),
        Number::from_ratio_u256(15u64, 100u64)
    );
    assert_eq!(
        pos.take_profit_price_total(&price_point, MarketType::CollateralIsQuote)
            .unwrap()
            .unwrap()
            .into_number(),
        Number::from_ratio_u256(3u64, 40u64)
    );
}

#[test]
fn position_data_infinite_max_gains() {
    let price = Price::try_from_number(Number::from_ratio_u256(1u64, 10u64)).unwrap();
    let market_type = MarketType::CollateralIsBase;
    let price_point = PricePoint {
        price_notional: Price::try_from_number(Number::from_ratio_u256(1u64, 10u64)).unwrap(),
        price_usd: PriceCollateralInUsd::try_from_number(Number::from_ratio_u256(1u64, 10u64))
            .unwrap(),
        timestamp: Default::default(),
        price_base: price.into_base_price(market_type),
        is_notional_usd: true,
        market_type,
        publish_time: None,
        publish_time_usd: None,
    };

    let pos = Position {
        owner: Addr::unchecked("trader".to_string()),
        id: PositionId::new(123),
        deposit_collateral: SignedCollateralAndUsd::new("1000".parse().unwrap(), &price_point),
        active_collateral: "1000".parse().unwrap(),
        counter_collateral: "2000".parse().unwrap(),
        notional_size: "-20000".parse().unwrap(),
        created_at: Default::default(),
        price_point_created_at: Default::default(),
        trading_fee: Default::default(),
        funding_fee: Default::default(),
        borrow_fee: Default::default(),
        crank_fee: Default::default(),
        delta_neutrality_fee: Default::default(),
        liquifunded_at: Default::default(),
        next_liquifunding: Default::default(),
        stop_loss_override: None,
        take_profit_trader: None,
        liquidation_margin: Default::default(),
        liquidation_price: None,
        take_profit_total: None,
        stop_loss_override_notional: None,
        take_profit_trader_notional: None,
    };

    // infinity max gains in notional asset
    assert_eq!(
        pos.take_profit_price_total(&price_point, MarketType::CollateralIsBase)
            .unwrap(),
        None
    );
}

#[test]
fn position_data_open_flip_short() {
    let pos = Position {
        owner: Addr::unchecked("trader".to_string()),
        id: PositionId::new(123),
        deposit_collateral: Default::default(),
        active_collateral: "100".parse().unwrap(),
        counter_collateral: "200".parse().unwrap(),
        notional_size: "-2".parse().unwrap(),
        created_at: Default::default(),
        price_point_created_at: Default::default(),
        trading_fee: Default::default(),
        funding_fee: Default::default(),
        borrow_fee: Default::default(),
        crank_fee: Default::default(),
        delta_neutrality_fee: Default::default(),
        liquifunded_at: Default::default(),
        next_liquifunding: Default::default(),
        stop_loss_override: None,
        take_profit_trader: None,
        liquidation_margin: Default::default(),
        liquidation_price: None,
        take_profit_total: None,
        stop_loss_override_notional: None,
        take_profit_trader_notional: None,
    };

    let price = Price::try_from_number(Number::from(300u64)).unwrap();
    let price_usd = PriceCollateralInUsd::try_from_number(Number::from(300u64)).unwrap();
    let market_type = MarketType::CollateralIsBase;
    let price_base = price.into_base_price(market_type);
    let entry_price = Price::try_from_number(Number::from(100u64)).unwrap();
    let pos_data = pos
        .into_query_response(
            PricePoint {
                price_notional: price,
                price_usd,
                timestamp: Default::default(),
                price_base,
                is_notional_usd: true,
                market_type,
                publish_time: None,
                publish_time_usd: None,
            },
            entry_price,
            MarketType::CollateralIsBase,
            Signed::zero(),
        )
        .unwrap();

    let expected_notional_size: Number = "-2".parse().unwrap();
    let expected_counter_collateral: Number = "200".parse().unwrap();
    assert!(
        (expected_notional_size - pos_data.notional_size.into_number())
            .unwrap()
            .approx_eq(Number::ZERO)
            .unwrap(),
        "{} != {}",
        expected_notional_size,
        pos_data.notional_size
    );
    assert!(
        (expected_counter_collateral - pos_data.counter_collateral.into_number())
            .unwrap()
            .approx_eq(Number::ZERO)
            .unwrap(),
        "{} != {}",
        expected_counter_collateral,
        pos_data.counter_collateral
    );
}

#[test]
fn position_data_open_flip_long() {
    let pos = Position {
        owner: Addr::unchecked("trader".to_string()),
        id: PositionId::new(123),
        deposit_collateral: Default::default(),
        active_collateral: "100".parse().unwrap(),
        counter_collateral: "200".parse().unwrap(),
        notional_size: "2".parse().unwrap(),
        //direction: Direction::Long,
        created_at: Default::default(),
        price_point_created_at: Default::default(),
        trading_fee: Default::default(),
        funding_fee: Default::default(),
        borrow_fee: Default::default(),
        crank_fee: Default::default(),
        delta_neutrality_fee: Default::default(),
        liquifunded_at: Default::default(),
        next_liquifunding: Default::default(),
        stop_loss_override: None,
        take_profit_trader: None,
        liquidation_margin: Default::default(),
        liquidation_price: None,
        take_profit_total: None,
        stop_loss_override_notional: None,
        take_profit_trader_notional: None,
    };

    let price = Price::try_from_number(Number::from(300u64)).unwrap();
    let price_usd = PriceCollateralInUsd::try_from_number(Number::from(300u64)).unwrap();
    let entry_price = Price::try_from_number(Number::from(100u64)).unwrap();
    let market_type = MarketType::CollateralIsBase;
    let price_base = price.into_base_price(market_type);
    let pos_data = pos
        .into_query_response(
            PricePoint {
                price_notional: price,
                price_usd,
                timestamp: Default::default(),
                price_base,
                is_notional_usd: true,
                market_type,
                publish_time: None,
                publish_time_usd: None,
            },
            entry_price,
            MarketType::CollateralIsBase,
            Signed::zero(),
        )
        .unwrap();

    let expected_notional_size: Number = "2".parse().unwrap();
    let expected_counter_collateral: Number = "200".parse().unwrap();
    assert!(
        (expected_notional_size - pos_data.notional_size.into_number())
            .unwrap()
            .approx_eq(Number::ZERO)
            .unwrap(),
        "{} != {}",
        expected_notional_size,
        pos_data.notional_size
    );
    assert!(
        (expected_counter_collateral - pos_data.counter_collateral.into_number())
            .unwrap()
            .approx_eq(Number::ZERO)
            .unwrap(),
        "{} != {}",
        expected_counter_collateral,
        pos_data.counter_collateral
    );
}
