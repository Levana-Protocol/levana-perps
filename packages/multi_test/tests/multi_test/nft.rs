use cosmwasm_std::Decimal256;
use levana_perpswap_multi_test::{
    cw721_helpers::NftMetadataExt, market_wrapper::PerpsMarket, response::CosmosResponseExt,
    return_unless_market_collateral_quote, PerpsApp,
};
use msg::contracts::market::config::ConfigUpdate;
use msg::prelude::*;

#[test]
fn nft_position() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_quote!(market);

    market
        .exec_set_config(ConfigUpdate {
            trading_fee_notional_size: Some("0.001".parse().unwrap()),
            trading_fee_counter_collateral: Some("0.001".parse().unwrap()),
            delta_neutrality_fee_tax: Some(Decimal256::zero()),
            ..Default::default()
        })
        .unwrap();

    let trader = market.clone_trader(0).unwrap();

    let (_, defer_res) = market
        .exec_open_position(
            &trader,
            "100",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    let mut nft_ids = market.query_position_token_ids(&trader).unwrap();
    assert_eq!(nft_ids.len(), 1);

    let meta = market
        .query_position_token_metadata(&nft_ids.pop().unwrap())
        .unwrap();

    assert_eq!(
        Number::try_from(meta.get_attr("pos-active-collateral").unwrap()).unwrap(),
        Number::try_from("98.9").unwrap() - defer_res.exec_resp().first_delta_neutrality_fee_amount()
    );
}

#[test]
fn nft_transfer() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader_1 = market.clone_trader(1).unwrap();
    let trader_2 = market.clone_trader(2).unwrap();

    market
        .exec_open_position(
            &trader_1,
            "100",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    // confirm that owner is trader_1
    let mut nft_ids = market.query_position_token_ids(&trader_1).unwrap();
    assert_eq!(nft_ids.len(), 1);
    assert_eq!(0, market.query_position_token_ids(&trader_2).unwrap().len());

    let token_id = nft_ids.pop().unwrap();
    assert_eq!(
        trader_1,
        market.query_position_token_owner(&token_id).unwrap()
    );

    // transfer to trader_2

    market
        .exec_position_token_transfer(&token_id, &trader_1, &trader_2)
        .unwrap();

    // confirm that owner is trader_2
    let mut nft_ids = market.query_position_token_ids(&trader_2).unwrap();
    assert_eq!(nft_ids.len(), 1);
    assert_eq!(0, market.query_position_token_ids(&trader_1).unwrap().len());

    let token_id = nft_ids.pop().unwrap();
    assert_eq!(
        trader_2,
        market.query_position_token_owner(&token_id).unwrap()
    );

    // can no longer transfer from trader_1
    market
        .exec_position_token_transfer(&token_id, &trader_1, &trader_2)
        .unwrap_err();

    // but can transfer from trader_2
    market
        .exec_position_token_transfer(&token_id, &trader_2, &trader_1)
        .unwrap();
}

#[test]
fn nft_transfer_gate() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let trader_1 = market.clone_trader(1).unwrap();
    let trader_2 = market.clone_trader(2).unwrap();

    market
        .exec_open_position(
            &trader_1,
            "100",
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    // confirm that owner is trader_1
    let mut nft_ids = market.query_position_token_ids(&trader_1).unwrap();
    assert_eq!(nft_ids.len(), 1);
    assert_eq!(0, market.query_position_token_ids(&trader_2).unwrap().len());

    let token_id = nft_ids.pop().unwrap();
    assert_eq!(
        trader_1,
        market.query_position_token_owner(&token_id).unwrap()
    );

    // disable nft execution
    market
        .exec_set_config(ConfigUpdate {
            disable_position_nft_exec: Some(true),
            ..Default::default()
        })
        .unwrap();

    // should not be allowed to transfer to trader_2
    market
        .exec_position_token_transfer(&token_id, &trader_1, &trader_2)
        .unwrap_err();

    // confirm that owner is still trader_1
    let mut nft_ids = market.query_position_token_ids(&trader_1).unwrap();
    assert_eq!(nft_ids.len(), 1);
    assert_eq!(0, market.query_position_token_ids(&trader_2).unwrap().len());

    let token_id = nft_ids.pop().unwrap();
    assert_eq!(
        trader_1,
        market.query_position_token_owner(&token_id).unwrap()
    );

    // re-enable nft execution
    market
        .exec_set_config(ConfigUpdate {
            disable_position_nft_exec: Some(false),
            ..Default::default()
        })
        .unwrap();

    // can now transfer to trader_2
    market
        .exec_position_token_transfer(&token_id, &trader_1, &trader_2)
        .unwrap();

    // confirm that owner is trader_2
    let mut nft_ids = market.query_position_token_ids(&trader_2).unwrap();
    assert_eq!(nft_ids.len(), 1);
    assert_eq!(0, market.query_position_token_ids(&trader_1).unwrap().len());

    let token_id = nft_ids.pop().unwrap();
    assert_eq!(
        trader_2,
        market.query_position_token_owner(&token_id).unwrap()
    );
}
