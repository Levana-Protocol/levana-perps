use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, response::CosmosResponseExt,
    return_unless_market_collateral_quote, time::TimeJump, PerpsApp,
};
use msg::contracts::market::config::ConfigUpdate;
use msg::prelude::*;

#[test]
fn delta_neutrality_fee_cap() {
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

    market
        .exec_set_config(ConfigUpdate {
            delta_neutrality_fee_sensitivity: Some("2".try_into().unwrap()),
            ..ConfigUpdate::default()
        })
        .unwrap();

    // get expected error after updating
    // let err = market
    let _ = market
        .exec_update_position_collateral_impact_size(
            &trader,
            pos_id,
            "20".try_into().unwrap(),
            None,
        )
        .unwrap_err();

    //FIXME - restore precise error checking in test
    /*
    let err: PerpError<MarketError> = err
        .downcast()
        .unwrap();

    if err.id != ErrorId::DeltaNeutralityFeeAlreadyLong {
        panic!("{:?}", err);
    } 
    */
}

#[test]
fn artificial_slippage_charge_open_close_nochange() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    let (pos_id, defer_res) = market
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

    let open_amount = defer_res.exec_resp().first_delta_neutrality_fee_amount();
    assert_ne!(open_amount, Number::ZERO);

    let defer_res = market.exec_close_position(&trader, pos_id, None).unwrap();
    let close_amount = defer_res.exec_resp().first_delta_neutrality_fee_amount();

    // close should be exactly the inverse of open
    assert_eq!(close_amount, -open_amount);
}

#[test]
fn artificial_slippage_charge_update() {
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

    // updating without affecting size should not charge slippage
    let res = market
        .exec_update_position_collateral_impact_leverage(&trader, pos_id, "10".try_into().unwrap())
        .unwrap();
    res.try_first_delta_neutrality_fee_amount().unwrap_err();

    // updating with affecting size should though
    let res = market
        .exec_update_position_collateral_impact_size(
            &trader,
            pos_id,
            "10".try_into().unwrap(),
            None,
        )
        .unwrap();
    let update_amount = res.first_delta_neutrality_fee_amount();
    assert_ne!(update_amount, Number::ZERO);
}

#[test]
fn artificial_slippage_direction_messages_work() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&trader, "1000000000".parse().unwrap())
        .unwrap();

    let err = market
        .exec_open_position(
            &trader,
            "1000000",
            "20",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("protocol would become too long"),
        "Doesn't mention long: {err:?}"
    );

    let err = market
        .exec_open_position(
            &trader,
            "1000000",
            "20",
            DirectionToBase::Short,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("protocol would become too short"),
        "Doesn't mention short: {err:?}"
    );
}

#[derive(Clone, Copy, Debug)]
struct Params {
    open_long_first: bool,
    close_long_first: bool,
    new_sensitivity: &'static str,
    new_cap: &'static str,
}

fn mid_protocol_param_update_perp_843_helper(
    Params {
        open_long_first,
        close_long_first,
        new_sensitivity,
        new_cap,
    }: Params,
) -> anyhow::Result<()> {
    let market = PerpsMarket::new(PerpsApp::new_cell()?)?;
    let trader_long = market.clone_trader(0)?;
    let trader_short = market.clone_trader(1)?;
    let lp = market.clone_lp(0)?;

    market.exec_mint_and_deposit_liquidity(&lp, "1000000000".parse()?)?;

    let open_long = || {
        market.exec_open_position(
            &trader_long,
            "100",
            "20",
            DirectionToBase::Long,
            "2.0",
            None,
            None,
            None,
        )
    };
    let open_short = || {
        market.exec_open_position(
            &trader_short,
            "100",
            "20",
            DirectionToBase::Short,
            "2.0",
            None,
            None,
            None,
        )
    };

    let (pos_long, pos_short) = if open_long_first {
        let (pos_long, _) = open_long()?;
        let (pos_short, _) = open_short()?;
        (pos_long, pos_short)
    } else {
        let (pos_short, _) = open_short()?;
        let (pos_long, _) = open_long()?;
        (pos_long, pos_short)
    };

    market.set_time(TimeJump::Blocks(2))?;
    market.exec_set_config(ConfigUpdate {
        delta_neutrality_fee_sensitivity: Some(new_sensitivity.parse()?),
        delta_neutrality_fee_cap: Some(new_cap.parse()?),
        ..Default::default()
    })?;
    market.exec_crank_till_finished(&lp)?;

    if close_long_first {
        market.exec_close_position(&trader_long, pos_long, None)?;
        market.exec_close_position(&trader_short, pos_short, None)?;
    } else {
        market.exec_close_position(&trader_short, pos_short, None)?;
        market.exec_close_position(&trader_long, pos_long, None)?;
    }
    market.exec_withdraw_liquidity(&lp, None)?;
    // NOTE: We rely on the sanity checks to ensure that the protocol remained
    // solvent during these interactions.
    Ok(())
}

#[test]
fn mid_protocol_param_update_perp_843() {
    for open_long_first in [true, false] {
        for close_long_first in [true, false] {
            for new_sensitivity in [
                "50000",
                "500000",
                "5000000",
                "50000000",
                "500000000",
                "5000000000",
            ] {
                for new_cap in ["0.005", "0.01", "0.05", "0.1", "0.2"] {
                    let params = Params {
                        open_long_first,
                        close_long_first,
                        new_sensitivity,
                        new_cap,
                    };
                    mid_protocol_param_update_perp_843_helper(params)
                        .with_context(|| format!("Failed with parameters: {params:?}"))
                        .unwrap();
                }
            }
        }
    }
}

#[test]
fn negative_in_delta_neutrality_fee_perp_986() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader_long = market.clone_trader(0).unwrap();
    let trader_short = market.clone_trader(1).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&lp, "1000000".parse().unwrap())
        .unwrap();

    let open_long = |collateral: &str| {
        market.exec_open_position(
            &trader_long,
            collateral,
            "10",
            DirectionToBase::Long,
            "2.0",
            None,
            None,
            None,
        )?;
        market.set_time(TimeJump::Blocks(5))?;
        market.exec_crank_till_finished(&lp)
    };
    let open_short = |collateral: &str| {
        market.exec_open_position(
            &trader_short,
            collateral,
            "8",
            DirectionToBase::Short,
            "2.0",
            None,
            None,
            None,
        )?;
        market.set_time(TimeJump::Blocks(5))?;
        market.exec_crank_till_finished(&lp)
    };

    open_short("1000.001").unwrap();
    open_long("1000").unwrap();
    open_long("10").unwrap();
}

#[test]
fn negative_in_delta_neutrality_fee_single_update_perp_986() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market
        .exec_set_config(ConfigUpdate {
            minimum_deposit_usd: Some(Usd::zero()),
            ..Default::default()
        })
        .unwrap();
    let trader_long = market.clone_trader(0).unwrap();
    let trader_short = market.clone_trader(1).unwrap();
    let lp = market.clone_lp(0).unwrap();

    market
        .exec_mint_and_deposit_liquidity(&lp, "1000000".parse().unwrap())
        .unwrap();

    let open_long = |collateral: &str| {
        market.exec_open_position(
            &trader_long,
            collateral,
            "10",
            DirectionToBase::Long,
            "2.0",
            None,
            None,
            None,
        )?;
        market.set_time(TimeJump::Blocks(5))?;
        market.exec_crank_till_finished(&lp)
    };
    let open_short = |collateral: &str| {
        market.exec_open_position(
            &trader_short,
            collateral,
            "8",
            DirectionToBase::Short,
            "2.0",
            None,
            None,
            None,
        )?;
        market.set_time(TimeJump::Blocks(5))?;
        market.exec_crank_till_finished(&lp)
    };

    open_short("1000.001").unwrap();
    open_long("1000").unwrap();
    open_long("0.001").unwrap();
}

#[test]
fn artificial_slippage_charge_change_net_notional_sign() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_quote!(market);
    let trader = market.clone_trader(0).unwrap();
    market
        .exec_set_config(ConfigUpdate {
            minimum_deposit_usd: Some(Usd::zero()),
            delta_neutrality_fee_tax: Some(Decimal256::zero()),
            ..Default::default()
        })
        .unwrap();

    let (pos_id, defer_res) = market
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

    let open_amount1 = defer_res.exec_resp().first_delta_neutrality_fee_amount();
    let status1 = market.query_status().unwrap();
    let net_notional1 = status1.long_notional - status1.short_notional;
    market.exec_close_position(&trader, pos_id, None).unwrap();

    let (_, _) = market
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

    let open_amount2 = defer_res.exec_resp().first_delta_neutrality_fee_amount();

    let (_, _) = market
        .exec_open_position(
            &trader,
            "200",
            "10",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let open_amount3 = defer_res.exec_resp().first_delta_neutrality_fee_amount();
    let status2 = market.query_status().unwrap();
    let net_notional2 = status2.long_notional - status2.short_notional;
    assert_eq!(net_notional1, net_notional2);
    assert_eq!(open_amount1, open_amount2 + open_amount3);
}
