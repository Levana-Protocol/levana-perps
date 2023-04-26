use std::str::FromStr;

use cosmwasm_std::{Addr, Decimal256, Uint256};
use levana_perpswap_multi_test::config::DEFAULT_MARKET;
use levana_perpswap_multi_test::return_unless_market_collateral_quote;
use levana_perpswap_multi_test::time::TimeJump;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::contracts::cw20::entry::{QueryMsg as Cw20QueryMsg, TokenInfoResponse};
use msg::contracts::liquidity_token::LiquidityTokenKind;
use msg::contracts::market::config::ConfigUpdate;
use msg::contracts::market::liquidity::LiquidityStats;
use msg::prelude::*;

#[test]
fn liquidity_deposit_new_user() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let initial_liquidity_stats = market.query_liquidity_stats().unwrap();
    let amount = Number::from(100u64);

    let new_lp = Addr::unchecked("new_lp");
    market
        .exec_mint_and_deposit_liquidity(&new_lp, amount)
        .unwrap();

    let new_liquidity_stats = market.query_liquidity_stats().unwrap();
    assert_eq!(
        new_liquidity_stats,
        LiquidityStats {
            unlocked: initial_liquidity_stats.unlocked
                + Collateral::try_from_number(amount).unwrap(),
            ..new_liquidity_stats
        }
    );

    let shares = market.query_lp_info(&new_lp).unwrap().lp_amount;
    assert_eq!(shares.into_number(), amount);
}

#[test]
fn liquidity_withdraw_new_user() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let new_lp = Addr::unchecked("new-lp");

    // Mint & Deposit

    market
        .exec_mint_tokens(&new_lp, Number::from(1000u64))
        .unwrap();
    let start_balance = market.query_collateral_balance(&new_lp).unwrap();

    let deposit_amount = Number::from(100u64);
    let withdraw_amount = Number::from(50u64);
    market
        .exec_deposit_liquidity(&new_lp, deposit_amount)
        .unwrap();

    let liquidity_stats_after_deposit = market.query_liquidity_stats().unwrap();

    // Withdraw
    market
        .exec_withdraw_liquidity(&new_lp, Some(withdraw_amount))
        .unwrap();

    let liquidity_stats_after_withdraw = market.query_liquidity_stats().unwrap();
    let end_balance = market.query_collateral_balance(&new_lp).unwrap();

    // Assert

    let unlocked = deposit_amount - withdraw_amount;
    assert_eq!(start_balance - unlocked, end_balance);

    assert_eq!(
        LiquidityStats {
            unlocked: liquidity_stats_after_deposit.unlocked
                - Collateral::try_from_number(unlocked).unwrap(),
            total_lp: liquidity_stats_after_deposit.total_lp
                - LpToken::try_from_number(unlocked).unwrap(),
            locked: liquidity_stats_after_deposit.locked,
            total_xlp: liquidity_stats_after_deposit.total_xlp,
        },
        liquidity_stats_after_withdraw
    );
}

#[test]
fn liquidity_share_allocation_with_trading() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_quote!(market);

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();
    let initial_liquidity_stats = market.query_liquidity_stats().unwrap();

    let lp1 = Addr::unchecked("lp1");
    let lp2 = Addr::unchecked("lp2");
    let lp3 = Addr::unchecked("lp3");

    // Mint & Deposit separately

    market
        .exec_mint_tokens(&lp1, Number::from(1000u64))
        .unwrap();
    market
        .exec_mint_tokens(&lp2, Number::from(1000u64))
        .unwrap();
    market
        .exec_mint_tokens(&lp3, Number::from(1000u64))
        .unwrap();

    market.exec_deposit_liquidity(&lp1, 100u64.into()).unwrap();
    market.exec_deposit_liquidity(&lp2, 200u64.into()).unwrap();
    market.exec_deposit_liquidity(&lp3, 300u64.into()).unwrap();

    // Open position & liquidate

    market.exec_set_price("100".try_into().unwrap()).unwrap();

    let collateral = Number::from(100u64);
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            collateral,
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();

    let liquidity_stats_pre_liquidation = market.query_liquidity_stats().unwrap();
    market.exec_set_price("1".try_into().unwrap()).unwrap();

    // Assert pre-liquidation
    assert_eq!(
        liquidity_stats_pre_liquidation.locked,
        initial_liquidity_stats.locked + Collateral::try_from_number(collateral).unwrap()
    );
    assert_eq!(
        liquidity_stats_pre_liquidation.unlocked,
        initial_liquidity_stats.unlocked + Collateral::from(500u64) // 500 == lp deposits - collateral
    );

    // Assert post-liquidation

    let _pos = market.query_position_pending_close(pos_id, true).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();
    let _pos = market.query_closed_position(&trader, pos_id).unwrap();

    let liquidity_stats_post_liquidation = market.query_liquidity_stats().unwrap();
    let total_shares = initial_liquidity_stats.unlocked + Collateral::from(600u64);
    let total_liquidity = liquidity_stats_post_liquidation.unlocked;

    let assert_lp = |lp: &Addr, shares: Number| {
        let start_balance = market.query_collateral_balance(lp).unwrap();

        market.exec_withdraw_liquidity(lp, None).unwrap();

        let end_balance = market.query_collateral_balance(lp).unwrap();
        let actual_return = end_balance - start_balance;
        let expected_return =
            total_liquidity.into_number() / total_shares.into_number() * shares.into_number();

        assert_eq!(
            actual_return.to_u128_with_precision(6),
            expected_return.to_u128_with_precision(6)
        );
    };

    assert_lp(&lp1, Number::from(100u64));
    assert_lp(&lp2, Number::from(200u64));
    assert_lp(&lp3, Number::from(300u64));
}

#[test]
fn liquidity_claim_yield_from_borrow_fee() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_quote!(market);

    // Ensure a fixed borrow fee rate to simplify calculations here
    market
        .exec_set_config(ConfigUpdate {
            borrow_fee_rate_min_annualized: Some("0.01".parse().unwrap()),
            borrow_fee_rate_max_annualized: Some("0.01".parse().unwrap()),
            delta_neutrality_fee_tax: Some(Decimal256::zero()),
            ..Default::default()
        })
        .unwrap();

    let trader = market.clone_trader(0).unwrap();
    let cranker = market.clone_trader(1).unwrap();
    let lp1 = Addr::unchecked("lp1");

    // Mint & Deposit separately

    market
        .exec_mint_tokens(&lp1, Number::from(1100u64))
        .unwrap();

    market.exec_deposit_liquidity(&lp1, 1000u64.into()).unwrap();

    // Open position

    let collateral = Number::from(100u64);
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            collateral,
            "10",
            DirectionToBase::Long,
            "1",
            None,
            None,
            None,
        )
        .unwrap();
    // Trigger liquifunding

    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    // Calculate trading fee

    let config = market.query_config().unwrap();
    let pos = market.query_position(pos_id).unwrap();
    let mut trading_fee = pos.notional_size_in_collateral.into_number()
        * config.trading_fee_notional_size.into_number();
    trading_fee +=
        pos.counter_collateral.into_number() * config.trading_fee_counter_collateral.into_number();

    // Calculate borrow fee

    const NS_PER_YEAR: u128 = 31_536_000_000_000_000u128;

    let rates = market.query_status().unwrap();
    let delay_nanos = Duration::from_seconds(config.liquifunding_delay_seconds as u64).as_nanos();
    let accumulated_rate = rates.borrow_fee.into_number() * delay_nanos;
    let borrow_fee =
        accumulated_rate * pos.counter_collateral.into_number() / Number::from(NS_PER_YEAR);

    // Assert

    let trading_fee_yield = trading_fee / Number::from(4u64);
    let borrow_fee_yield = borrow_fee / Number::from(4u64);
    let expected_yield = (borrow_fee_yield + trading_fee_yield)
        .checked_mul_number("0.7".parse().unwrap())
        .unwrap() // take protocol tax
        .to_u128_with_precision(6)
        .unwrap();

    let wallet_balance_before_claim = market.query_collateral_balance(&lp1).unwrap();
    market.exec_claim_yield(&lp1).unwrap();
    let wallet_balance_after_claim = market.query_collateral_balance(&lp1).unwrap();

    let actual_yield = wallet_balance_after_claim - wallet_balance_before_claim;
    let actual_yield = actual_yield.to_u128_with_precision(6).unwrap();

    assert_eq!(actual_yield, expected_yield);
}

#[test]
fn liquidity_token_transfer() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let lp_contract = market
        .query_liquidity_token_addr(LiquidityTokenKind::Lp)
        .unwrap();
    let initial_liquidity_stats = market.query_liquidity_stats().unwrap();

    let new_lp = Addr::unchecked("new-lp");
    let amount = Number::from(100u64);

    market
        .exec_mint_and_deposit_liquidity(&new_lp, amount)
        .unwrap();

    let new_liquidity_stats = market.query_liquidity_stats().unwrap();
    assert_eq!(
        new_liquidity_stats,
        LiquidityStats {
            unlocked: initial_liquidity_stats.unlocked
                + Collateral::try_from_number(amount).unwrap(),
            ..new_liquidity_stats
        }
    );

    let shares = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(shares.lp_amount, LpToken::try_from_number(amount).unwrap());
    assert_eq!(shares.xlp_amount, LpToken::zero());

    let cw20_info: TokenInfoResponse = market
        .query_liquidity_token(LiquidityTokenKind::Lp, &Cw20QueryMsg::TokenInfo {})
        .unwrap();
    let decimals = cw20_info.decimals as u32;

    // get balance at start
    let cw20_balance = market
        .query_liquidity_token_balance_raw(LiquidityTokenKind::Lp, &new_lp)
        .unwrap();
    let start_balance = Number::from_fixed_u128(cw20_balance.into(), decimals);
    assert_eq!(start_balance, amount);

    // transfer
    let joe_shmoe = Addr::unchecked("joe-shmoe");
    let transfer_amount = Number::from(30u64);

    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Lp, &new_lp, &joe_shmoe, transfer_amount)
        .unwrap();

    // get balance after transfer
    let cw20_balance = market
        .query_liquidity_token_balance_raw(LiquidityTokenKind::Lp, &new_lp)
        .unwrap();
    let transfer_balance_lp = Number::from_fixed_u128(cw20_balance.into(), decimals);
    let cw20_balance = market
        .query_liquidity_token_balance_raw(LiquidityTokenKind::Lp, &joe_shmoe)
        .unwrap();
    let transfer_balance_joe = Number::from_fixed_u128(cw20_balance.into(), decimals);

    assert_eq!(transfer_balance_lp, amount - transfer_amount);
    assert_eq!(transfer_balance_joe, transfer_amount);

    // transfer back (deliberately use cw20 message, not liquidity token)
    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Lp, &joe_shmoe, &new_lp, transfer_amount)
        .unwrap();

    // get balance at end
    let cw20_balance = market
        .query_liquidity_token_balance_raw(LiquidityTokenKind::Lp, &new_lp)
        .unwrap();
    let end_balance_lp = Number::from_fixed_u128(cw20_balance.into(), decimals);
    let cw20_balance = market
        .query_liquidity_token_balance_raw(LiquidityTokenKind::Lp, &joe_shmoe)
        .unwrap();
    let end_balance_joe = Number::from_fixed_u128(cw20_balance.into(), decimals);

    assert_eq!(end_balance_lp, amount);
    assert_eq!(end_balance_joe, Number::ZERO);

    //test that we cannot go below zero
    let err = market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Lp,
            &joe_shmoe,
            &lp_contract,
            transfer_amount,
        )
        .unwrap_err();
    // confirm that we get the expected error
    let err: PerpError = err.downcast().unwrap();
    if err.id != ErrorId::Cw20Funds || err.domain != ErrorDomain::LiquidityToken {
        panic!("wrong error type!");
    }
}

#[test]
fn liquidity_stake_xlp() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let new_lp = Addr::unchecked("new-lp");

    let info = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(info.lp_amount, LpToken::zero(),);
    assert_eq!(info.xlp_amount, LpToken::zero(),);

    let amount = Number::from(100u64);

    // No LP tokens yet, cannot stake
    market.exec_stake_lp(&new_lp, None).unwrap_err();
    market.exec_stake_lp(&new_lp, Some(amount)).unwrap_err();

    // Deposit some collateral, get LP tokens
    market
        .exec_mint_and_deposit_liquidity(&new_lp, amount)
        .unwrap();

    let info = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(info.xlp_amount, LpToken::zero());
    assert!(info.lp_amount > LpToken::zero());

    // Staking more than we have should still fail
    market
        .exec_stake_lp(&new_lp, Some(info.lp_amount.into_number() + Number::ONE))
        .unwrap_err();

    // But staking less than we have should work
    let to_stake = info.lp_amount.into_number() / 2;
    let remaining_lp = info.lp_amount.into_number() - to_stake; // to deal with rounding
    market.exec_stake_lp(&new_lp, Some(to_stake)).unwrap();

    let info = market.query_lp_info(&new_lp).unwrap();
    assert!(info.xlp_amount > LpToken::zero());
    assert_eq!(info.lp_amount.into_number(), remaining_lp);

    // Can stake everything left
    market.exec_stake_lp(&new_lp, None).unwrap();
    let info = market.query_lp_info(&new_lp).unwrap();
    assert!(info.xlp_amount > LpToken::zero());
    assert_eq!(info.lp_amount, LpToken::zero());

    // But can't do it a second time
    market.exec_stake_lp(&new_lp, None).unwrap_err();
}

#[test]
fn liquidity_unstake_xlp() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let new_lp = Addr::unchecked("new-lp");

    // Capture initial stats
    let init_stats = market.query_liquidity_stats().unwrap();

    let info = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(info.lp_amount, LpToken::zero());
    assert_eq!(info.xlp_amount, LpToken::zero());

    let amount = Number::from(100u64);

    // No xLP tokens yet, cannot unstake
    market.exec_unstake_xlp(&new_lp, Some(amount)).unwrap_err();
    market.exec_unstake_xlp(&new_lp, None).unwrap_err();

    // Deposit some collateral, get LP tokens
    market
        .exec_mint_and_deposit_liquidity(&new_lp, amount)
        .unwrap();

    let info = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(info.xlp_amount, LpToken::zero());
    assert!(info.lp_amount > LpToken::zero());

    // Still no xLP, still cannot unstake
    market.exec_unstake_xlp(&new_lp, Some(amount)).unwrap_err();
    market.exec_unstake_xlp(&new_lp, None).unwrap_err();

    // Stake everything into xLP
    market.exec_stake_lp(&new_lp, None).unwrap();

    let new_info = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(new_info.xlp_amount, info.lp_amount);
    assert_eq!(new_info.lp_amount, LpToken::zero());

    let stats = market.query_liquidity_stats().unwrap();
    assert_eq!(stats.total_lp, init_stats.total_lp);
    assert_eq!(stats.total_xlp, new_info.xlp_amount);
    assert_eq!(
        stats.unlocked.into_number(),
        init_stats.unlocked.into_number() + amount
    );

    // Unstaking more than we have fails
    market
        .exec_unstake_xlp(
            &new_lp,
            Some(new_info.xlp_amount.into_number() + Number::ONE),
        )
        .unwrap_err();

    // Now unstake half our xLP
    market
        .exec_unstake_xlp(
            &new_lp,
            Some(stats.total_xlp.into_number() / Number::from(2u64)),
        )
        .unwrap();

    // Unstaking everything will succeed. It will end up resetting the unstaking period.
    market.exec_unstake_xlp(&new_lp, None).unwrap();

    // And doing it again _also_ succeeds, because we haven't collected everything yet
    market.exec_unstake_xlp(&new_lp, None).unwrap();

    // We should still have some xLP right now

    // Wait for it all to unstake
    market.set_time(TimeJump::Hours(21 * 24)).unwrap();

    // Now claim everything and make sure we have no xLP left
    market.exec_collect_unstaked_lp(&new_lp).unwrap();
    let shares = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(shares.xlp_amount, LpToken::zero());
    assert_ne!(shares.lp_amount, LpToken::zero());
    assert_eq!(shares.unstaking, None);

    // And collecting a second time should fail
    market.exec_collect_unstaked_lp(&new_lp).unwrap_err();
}

#[test]
fn test_collect_unlocked_lp() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let new_lp = Addr::unchecked("new-lp");
    let amount = Number::from(100u64);

    market.automatic_time_jump_enabled = false;

    // Error scenario
    market.exec_collect_unstaked_lp(&new_lp).unwrap_err();

    // Deposit, stake, and unstake half

    market
        .exec_mint_and_deposit_liquidity(&new_lp, amount)
        .unwrap();

    market.exec_stake_lp(&new_lp, Some(amount)).unwrap();
    market
        .exec_unstake_xlp(&new_lp, Some(Number::from(50u64)))
        .unwrap();

    // Move ahead halfway and collect

    let config = market.query_config().unwrap();
    let interval = config.unstake_period_seconds / 2;
    market.set_time(TimeJump::Seconds(interval.into())).unwrap();

    let unlocked_lp = market
        .query_lp_info(&new_lp)
        .unwrap()
        .unstaking
        .unwrap()
        .available;
    assert_eq!(unlocked_lp, 25u64.into());

    market.exec_collect_unstaked_lp(&new_lp).unwrap();
    let info = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(info.lp_amount, 25u64.into());
    assert_eq!(info.xlp_amount, 75u64.into());
    assert_eq!(info.unstaking.as_ref().unwrap().collected, 25u64.into());
    assert_eq!(info.unstaking.as_ref().unwrap().available, 0u64.into());
    assert_eq!(info.unstaking.as_ref().unwrap().pending, 25u64.into());

    // Move ahead all the way and collect
    market.set_time(TimeJump::Seconds(interval.into())).unwrap();
    market.exec_collect_unstaked_lp(&new_lp).unwrap();
    let info = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(info.lp_amount, 50u64.into());

    // Unstake the rest

    market.exec_unstake_xlp(&new_lp, None).unwrap();
    market
        .set_time(TimeJump::Seconds(config.unstake_period_seconds.into()))
        .unwrap();

    market.exec_collect_unstaked_lp(&new_lp).unwrap();
    let info = market.query_lp_info(&new_lp).unwrap();
    assert_eq!(info.lp_amount, 100u64.into());
}

#[test]
fn liquidity_xlp_receives_rewards() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Get some xLP and no LP
    let new_lp = Addr::unchecked("new-lp");
    let amount = Number::from(100u64);
    market
        .exec_mint_and_deposit_liquidity(&new_lp, amount)
        .unwrap();
    market.exec_stake_lp(&new_lp, None).unwrap();
    let info = market.query_lp_info(&new_lp).unwrap();
    assert!(info.xlp_amount > LpToken::zero());
    assert_eq!(info.lp_amount, LpToken::zero());

    // No yield available initially
    assert_eq!(
        market.query_lp_info(&new_lp).unwrap().available_yield,
        Collateral::zero()
    );

    // Have a trader open a position and keep it running for a while for borrow fees
    let trader = market.clone_trader(0).unwrap();
    let _ = market
        .exec_open_position(
            &trader,
            "10",
            "10",
            DirectionToBase::Long,
            "5",
            None,
            None,
            None,
        )
        .unwrap();

    for _ in 0..100 {
        market.set_time(TimeJump::Hours(6)).unwrap();
        market.exec_refresh_price().unwrap();
        market.exec_crank_till_finished(&trader).unwrap();
    }

    // Should have some yield
    let resp = market.query_lp_info(&new_lp).unwrap();
    assert_ne!(resp.available_yield, Collateral::zero());
}

#[test]
fn perp_699_negative_xlp_fees() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Get some xLP and no LP
    let new_lp = Addr::unchecked("new-lp");
    let amount = Number::from(100u64);
    market
        .exec_mint_and_deposit_liquidity(&new_lp, amount)
        .unwrap();
    market.exec_stake_lp(&new_lp, None).unwrap();

    // Have a trader open a position and keep it running for a while for borrow fees
    let trader = market.clone_trader(0).unwrap();
    let _ = market
        .exec_open_position(
            &trader,
            "10",
            "10",
            DirectionToBase::Long,
            "5",
            None,
            None,
            None,
        )
        .unwrap();

    market
        .exec_mint_and_deposit_liquidity(&new_lp, Number::ONE)
        .unwrap();
    market.exec_claim_yield(&new_lp).unwrap();
}

#[test]
fn lp_info_api() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Get some xLP and no LP
    let new_lp = Addr::unchecked("new-lp");
    let amount = Number::from(100u64);
    market
        .exec_mint_and_deposit_liquidity(&new_lp, amount)
        .unwrap();
    market.exec_stake_lp(&new_lp, None).unwrap();
    let info = market.query_lp_info(&new_lp).unwrap();
    assert!(info.xlp_amount > LpToken::zero());
    assert_eq!(info.lp_amount, LpToken::zero());

    // No yield available initially
    assert_eq!(info.available_yield, Collateral::zero());

    // Have a trader open a position and keep it running for a while for borrow fees
    let trader = market.clone_trader(0).unwrap();
    let _ = market
        .exec_open_position(
            &trader,
            "10",
            "10",
            DirectionToBase::Long,
            "5",
            None,
            None,
            None,
        )
        .unwrap();

    for _ in 0..100 {
        market.set_time(TimeJump::Hours(6)).unwrap();
        market.exec_refresh_price().unwrap();
        market.exec_crank_till_finished(&trader).unwrap();
    }

    // Should have some yield
    assert_ne!(
        market.query_lp_info(&new_lp).unwrap().available_yield,
        Collateral::zero()
    );

    // Sanity check
    market.query_lp_info(&new_lp).unwrap();
    market
        .exec_deposit_liquidity_full(&new_lp, Number::ONE, true)
        .unwrap();

    market.exec_reinvest_yield(&new_lp, true).unwrap();
    market.exec_reinvest_yield(&new_lp, false).unwrap_err();
}

#[test]
fn lp_info_unknown_lp() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let addr = Addr::unchecked("unknown-lp");

    // This shouldn't fail
    market.query_lp_info(&addr).unwrap();
}

#[test]
fn drain_all_liquidity_perp_705() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Open a position to lock some liquidity
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

    // Withdraw the extra liquidity and make sure there is no unlocked
    // liquidity.  Now our position has all the liquidity in the pool.
    let lp = &DEFAULT_MARKET.bootstrap_lp_addr;
    let stats = market.query_liquidity_stats().unwrap();
    market
        .exec_withdraw_liquidity(lp, Some(stats.unlocked.into_number()))
        .unwrap();
    let stats = market.query_liquidity_stats().unwrap();
    assert_eq!(stats.unlocked, Collateral::zero());
    market.exec_claim_yield(lp).unwrap();
    market.exec_claim_yield(lp).unwrap_err();

    // Force a take profit on the position
    market.exec_set_price("10000".parse().unwrap()).unwrap();
    // Crank until we realize we need to reset LP balances
    for _ in 0..100 {
        market.exec_crank_single(&Addr::unchecked("crank")).unwrap();
        let crank_stats = market.query_crank_stats().unwrap();
        if crank_stats == Some(msg::contracts::market::crank::CrankWorkInfo::ResetLpBalances {}) {
            break;
        }
    }

    // Ensure the position is closed
    let _res = market.query_closed_position(&trader, pos_id).unwrap();

    // We should be blocked from depositing liquidity right now
    market
        .exec_mint_and_deposit_liquidity(lp, Number::from(5u64))
        .unwrap_err();

    // Now crank till all balances are reset
    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();

    // Ensure we have no liquidity left
    let stats = market.query_liquidity_stats().unwrap();
    assert_eq!(
        stats,
        LiquidityStats {
            locked: Collateral::zero(),
            unlocked: Collateral::zero(),
            total_lp: LpToken::zero(),
            total_xlp: LpToken::zero()
        }
    );

    // We should have a small amount of borrow fee received from the previous generation
    market.exec_claim_yield(lp).unwrap();
    // But as usual it should fail the second time through
    market.exec_claim_yield(lp).unwrap_err();

    // Ensure we can deposit liquidity again
    let amount = 5u64;
    market
        .exec_mint_and_deposit_liquidity(lp, Number::from(amount))
        .unwrap();
    let stats = market.query_liquidity_stats().unwrap();
    assert_eq!(stats.unlocked, Collateral::from(amount));
    assert_eq!(stats.total_lp, LpToken::from(amount));
    let info = market.query_lp_info(lp).unwrap();
    assert_eq!(info.lp_amount, LpToken::from(amount));
    assert_eq!(info.xlp_amount, LpToken::zero());
}

#[test]
fn lp_info_during_unstake_perp_736() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.automatic_time_jump_enabled = false;

    let addr = Addr::unchecked("liquidity-provider");

    market
        .exec_mint_and_deposit_liquidity(&addr, "12".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&addr, None).unwrap();
    let lp_info = market.query_lp_info(&addr).unwrap();
    assert_eq!(lp_info.lp_amount, "0".parse().unwrap());
    assert_eq!(lp_info.xlp_amount, "12".parse().unwrap());

    market
        .exec_unstake_xlp(&addr, Some("10".parse().unwrap()))
        .unwrap();

    let lp_info = market.query_lp_info(&addr).unwrap();
    assert_eq!(lp_info.lp_amount, "0".parse().unwrap());
    assert_eq!(lp_info.xlp_amount, "12".parse().unwrap());

    let mut last_xlp_pending = "10".parse().unwrap();
    assert_eq!(last_xlp_pending, lp_info.unstaking.unwrap().pending);
    for _ in 0..100 {
        market.set_time(TimeJump::Hours(2)).unwrap();
        let lp_info = market.query_lp_info(&addr).unwrap();
        assert_ne!(lp_info.lp_amount, "0".parse().unwrap());
        assert_eq!(
            LpToken::from_str("12").unwrap(),
            lp_info.lp_amount + lp_info.xlp_amount,
            "LP + xLP is not 12: {lp_info:?}"
        );
        let new_pending = lp_info.unstaking.unwrap().pending;
        assert!(last_xlp_pending > new_pending);
        last_xlp_pending = new_pending;

        // We left behind 2 xLP tokens, so the pending amount plus those 2
        // should always be the total xLP available.
        assert_eq!(
            new_pending,
            lp_info.xlp_amount - LpToken::from_str("2").unwrap()
        );
    }
}

#[test]
fn lp_info_partial_collection_perp_802() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let addr = Addr::unchecked("liquidity-provider");

    market
        .exec_mint_and_deposit_liquidity(&addr, "12".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&addr, None).unwrap();
    market
        .exec_unstake_xlp(&addr, Some("6".parse().unwrap()))
        .unwrap();

    market.set_time(TimeJump::Hours(1)).unwrap();
    market.exec_collect_unstaked_lp(&addr).unwrap();
    market.set_time(TimeJump::Hours(24 * 21)).unwrap();

    let lp_info = market.query_lp_info(&addr).unwrap();
    assert_eq!(lp_info.unstaking.unwrap().pending, LpToken::zero());
}

#[test]
fn direct_query_matches_lp_info_perp_827() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let addr = Addr::unchecked("liquidity-provider");

    market
        .exec_mint_and_deposit_liquidity(&addr, "12".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&addr, None).unwrap();
    market
        .exec_unstake_xlp(&addr, Some("6".parse().unwrap()))
        .unwrap();

    market.set_time(TimeJump::Hours(1)).unwrap();
    market.exec_collect_unstaked_lp(&addr).unwrap();
    market.set_time(TimeJump::Hours(24 * 21)).unwrap();

    let lp_info = market.query_lp_info(&addr).unwrap();
    assert_eq!(lp_info.unstaking.unwrap().pending, "0".parse().unwrap());

    assert_eq!(
        market
            .query_liquidity_token_balance_raw(LiquidityTokenKind::Lp, &addr)
            .unwrap()
            .to_string(),
        (lp_info.lp_amount.into_decimal256().atomics() / Uint256::from(1_000_000_000_000u128))
            .to_string()
    );
    assert_eq!(
        market
            .query_liquidity_token_balance_raw(LiquidityTokenKind::Xlp, &addr)
            .unwrap()
            .to_string(),
        (lp_info.xlp_amount.into_decimal256().atomics() / Uint256::from(1_000_000_000_000u128))
            .to_string()
    );
}

#[test]
fn unstaking_available_until_collection() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let addr = Addr::unchecked("liquidity-provider");

    market
        .exec_mint_and_deposit_liquidity(&addr, "12".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&addr, None).unwrap();
    market
        .exec_unstake_xlp(&addr, Some("6".parse().unwrap()))
        .unwrap();

    market.set_time(TimeJump::Hours(1)).unwrap();
    market.exec_collect_unstaked_lp(&addr).unwrap();
    market.set_time(TimeJump::Hours(24 * 21)).unwrap();

    let lp_info = market.query_lp_info(&addr).unwrap();
    assert_ne!(lp_info.unstaking, None);

    market.exec_collect_unstaked_lp(&addr).unwrap();
    let lp_info = market.query_lp_info(&addr).unwrap();
    assert_eq!(lp_info.unstaking, None);
}

#[test]
fn liquidity_deposit_withdraw_fractional() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let new_lp = Addr::unchecked("new-lp");

    // Mint & Deposit

    market
        .exec_mint_tokens(&new_lp, Number::from(1000u64))
        .unwrap();

    let amount: Number = "15.2932215".parse().unwrap();
    market.exec_deposit_liquidity(&new_lp, amount).unwrap();

    // Withdrawing past the extra decimal places is an error - we only deposited slightly less
    market
        .exec_withdraw_liquidity(&new_lp, Some(amount))
        .unwrap_err();

    // Withdrawing the truncated amount works
    market
        .exec_withdraw_liquidity(&new_lp, Some("15.293221".parse().unwrap()))
        .unwrap();
}
