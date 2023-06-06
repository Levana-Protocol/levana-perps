use crate::prelude::*;

#[test]
fn deposit_xlp() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let lp = market.clone_lp(0).unwrap();

    // Get some xLP tokens
    market
        .exec_mint_and_deposit_liquidity(&lp, "1000".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&lp, None).unwrap();

    let info1 = market.query_lp_info(&lp).unwrap();
    let stats1 = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(info1.lp_amount, LpToken::zero());
    assert_ne!(info1.xlp_amount, LpToken::zero());
    assert_eq!(stats1.farming_tokens, FarmingToken::zero());

    // Start the lockdrop
    market.exec_farming_start_lockdrop(None).unwrap();
    // Finish lockdrop
    market.set_time(TimeJump::Hours(24 * 365)).unwrap();
    // Start the launch
    market.exec_farming_start_launch().unwrap();

    // Deposit the xLP in the farming contract
    market
        .exec_farming_deposit_xlp(
            &lp,
            NonZero::new(info1.xlp_amount + "1".parse().unwrap()).unwrap(),
        )
        .unwrap_err();
    market
        .exec_farming_deposit_xlp(&lp, NonZero::new(info1.xlp_amount).unwrap())
        .unwrap();
    market
        .exec_farming_deposit_xlp(&lp, NonZero::new(info1.xlp_amount).unwrap())
        .unwrap_err();

    let info2 = market.query_lp_info(&lp).unwrap();
    let stats2 = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(info2.lp_amount, LpToken::zero());
    assert_eq!(info2.xlp_amount, LpToken::zero());
    assert_ne!(stats2.farming_tokens, FarmingToken::zero());

    // Withdraw the funds back
    market
        .exec_farming_withdraw_xlp(
            &lp,
            Some(NonZero::new(stats2.farming_tokens + "1".parse().unwrap()).unwrap()),
        )
        .unwrap_err();
    market
        .exec_farming_withdraw_xlp(&lp, Some(NonZero::new(stats2.farming_tokens).unwrap()))
        .unwrap();
    market
        .exec_farming_withdraw_xlp(&lp, Some(NonZero::new(stats2.farming_tokens).unwrap()))
        .unwrap_err();

    let info3 = market.query_lp_info(&lp).unwrap();
    let stats3 = market.query_farming_farmer_stats(&lp).unwrap();
    assert_eq!(info3.lp_amount, LpToken::zero());
    assert_eq!(info3.xlp_amount, info1.xlp_amount);
    assert_eq!(stats3.farming_tokens, FarmingToken::zero());
}
