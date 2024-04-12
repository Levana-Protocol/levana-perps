use proptest::prelude::*;
use std::cell::RefCell;
use std::ops::Mul;
use std::rc::Rc;
use std::str::FromStr;

use cosmwasm_std::{Addr, Decimal256, Uint128, Uint256};
use levana_perpswap_multi_test::arbitrary::lp::data::LpYield;
use levana_perpswap_multi_test::config::{SpotPriceKind, TokenKind, DEFAULT_MARKET, TEST_CONFIG};
use levana_perpswap_multi_test::return_unless_market_collateral_quote;
use levana_perpswap_multi_test::time::TimeJump;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::contracts::cw20::entry::{QueryMsg as Cw20QueryMsg, TokenInfoResponse};
use msg::contracts::liquidity_token::LiquidityTokenKind;
use msg::contracts::market::config::ConfigUpdate;
use msg::contracts::market::entry::PositionsQueryFeeApproach;
use msg::contracts::market::liquidity::LiquidityStats;
use msg::contracts::market::position::{LiquidationReason, PositionCloseReason};
use msg::prelude::*;
use msg::token::TokenInit;

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
            unlocked: (initial_liquidity_stats.unlocked
                + Collateral::try_from_number(amount).unwrap())
            .unwrap(),
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

    let unlocked = (deposit_amount - withdraw_amount).unwrap();
    assert_eq!((start_balance - unlocked).unwrap(), end_balance);

    assert_eq!(
        LiquidityStats {
            unlocked: (liquidity_stats_after_deposit.unlocked
                - Collateral::try_from_number(unlocked).unwrap())
            .unwrap(),
            total_lp: (liquidity_stats_after_deposit.total_lp
                - LpToken::try_from_number(unlocked).unwrap())
            .unwrap(),
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
        Ok(liquidity_stats_pre_liquidation.locked),
        initial_liquidity_stats.locked + Collateral::try_from_number(collateral).unwrap()
    );
    assert_eq!(
        Ok(liquidity_stats_pre_liquidation.unlocked),
        initial_liquidity_stats.unlocked + Collateral::from(500u64) // 500 == lp deposits - collateral
    );

    // Assert post-liquidation

    let _pos = market
        .query_position_pending_close(pos_id, PositionsQueryFeeApproach::NoFees)
        .unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();
    let _pos = market.query_closed_position(&trader, pos_id).unwrap();

    let liquidity_stats_post_liquidation = market.query_liquidity_stats().unwrap();
    let total_shares = (initial_liquidity_stats.unlocked + Collateral::from(600u64)).unwrap();
    let total_liquidity = liquidity_stats_post_liquidation.unlocked;

    let assert_lp = |lp: &Addr, shares: Number| {
        let start_balance = market.query_collateral_balance(lp).unwrap();

        market.exec_withdraw_liquidity(lp, None).unwrap();

        let end_balance = market.query_collateral_balance(lp).unwrap();
        let actual_return = (end_balance - start_balance).unwrap();
        let expected_return = ((total_liquidity.into_number() / total_shares.into_number())
            .unwrap()
            * shares.into_number())
        .unwrap();

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
            // We need precise liquifunding periods for this test so remove randomization
            liquifunding_delay_fuzz_seconds: Some(0),
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
    let mut trading_fee = (pos.notional_size_in_collateral.into_number()
        * config.trading_fee_notional_size.into_number())
    .unwrap();
    trading_fee = (trading_fee
        + (pos.counter_collateral.into_number()
            * config.trading_fee_counter_collateral.into_number())
        .unwrap())
    .unwrap();

    // Calculate borrow fee

    const NS_PER_YEAR: u128 = 31_536_000_000_000_000u128;

    let rates = market.query_status().unwrap();
    let delay_nanos = Duration::from_seconds(config.liquifunding_delay_seconds as u64).as_nanos();
    let accumulated_rate = (rates.borrow_fee.into_number() * delay_nanos).unwrap();
    let borrow_fee = ((accumulated_rate * pos.counter_collateral.into_number()).unwrap()
        / Number::from(NS_PER_YEAR))
    .unwrap();

    // Assert

    let trading_fee_yield = (trading_fee / Number::from(4u64)).unwrap();
    let borrow_fee_yield = (borrow_fee / Number::from(4u64)).unwrap();
    let expected_yield = (borrow_fee_yield + trading_fee_yield)
        .unwrap()
        .checked_mul_number("0.7".parse().unwrap())
        .unwrap() // take protocol tax
        .to_u128_with_precision(6)
        .unwrap();

    let wallet_balance_before_claim = market.query_collateral_balance(&lp1).unwrap();
    market.exec_claim_yield(&lp1).unwrap();
    let wallet_balance_after_claim = market.query_collateral_balance(&lp1).unwrap();

    let actual_yield = (wallet_balance_after_claim - wallet_balance_before_claim).unwrap();
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
            unlocked: (initial_liquidity_stats.unlocked
                + Collateral::try_from_number(amount).unwrap())
            .unwrap(),
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

    assert_eq!(transfer_balance_lp, (amount - transfer_amount).unwrap());
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
        .exec_stake_lp(
            &new_lp,
            Some((info.lp_amount.into_number() + Number::ONE).unwrap()),
        )
        .unwrap_err();

    // But staking less than we have should work
    let to_stake = (info.lp_amount.into_number() / 2).unwrap();
    let remaining_lp = (info.lp_amount.into_number() - to_stake).unwrap(); // to deal with rounding
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
        (init_stats.unlocked.into_number() + amount).unwrap()
    );

    // Unstaking more than we have fails
    market
        .exec_unstake_xlp(
            &new_lp,
            Some((new_info.xlp_amount.into_number() + Number::ONE).unwrap()),
        )
        .unwrap_err();

    // Now unstake half our xLP
    market
        .exec_unstake_xlp(
            &new_lp,
            Some((stats.total_xlp.into_number() / Number::from(2u64)).unwrap()),
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
        .exec_mint_and_deposit_liquidity_full(&new_lp, Number::ONE, true)
        .unwrap();

    market.exec_reinvest_yield(&new_lp, None, true).unwrap();
    market
        .exec_reinvest_yield(&new_lp, None, false)
        .unwrap_err();
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
    let market_type = market.id.get_market_type();

    // Open a position to lock some liquidity
    let trader = market.clone_trader(0).unwrap();
    let (long_pos_id, _) = market
        .exec_open_position(
            &trader,
            "10",
            "3",
            DirectionToBase::Long,
            "0.11",
            None,
            None,
            None,
        )
        .unwrap();

    let short_leverage = match market_type {
        MarketType::CollateralIsQuote => "3",
        MarketType::CollateralIsBase => "1",
    };

    let (short_pos_id, _) = market
        .exec_open_position(
            &trader,
            "10",
            short_leverage,
            DirectionToBase::Short,
            "0.11",
            None,
            None,
            None,
        )
        .unwrap();

    let status = market.query_status().unwrap();
    // Make sure positions balance each other out so there is 0 min liquidity requirement.
    assert_eq!(status.long_notional, status.short_notional);

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

    // Force a take profit on both positions
    market.exec_set_price("1.2".parse().unwrap()).unwrap();
    market
        .exec_crank_till_finished(&Addr::unchecked("crank"))
        .unwrap();
    market.exec_set_price("0.8".parse().unwrap()).unwrap();
    // Crank until we realize we need to reset LP balances
    let mut found_reset = false;
    for _ in 0..100 {
        market.exec_crank_single(&Addr::unchecked("crank")).unwrap();
        let crank_stats = market.query_crank_stats().unwrap();
        if crank_stats == Some(msg::contracts::market::crank::CrankWorkInfo::ResetLpBalances {}) {
            found_reset = true;
            break;
        }
    }

    // Ensure the positions are closed with max gains
    let res = market.query_closed_position(&trader, long_pos_id).unwrap();
    assert_eq!(
        res.reason,
        PositionCloseReason::Liquidated(LiquidationReason::MaxGains)
    );
    let res = market.query_closed_position(&trader, short_pos_id).unwrap();
    assert_eq!(
        res.reason,
        PositionCloseReason::Liquidated(LiquidationReason::MaxGains)
    );

    assert!(found_reset);

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
            (lp_info.lp_amount + lp_info.xlp_amount).unwrap(),
            "LP + xLP is not 12: {lp_info:?}"
        );
        let new_pending = lp_info.unstaking.unwrap().pending;
        assert!(last_xlp_pending > new_pending);
        last_xlp_pending = new_pending;

        // We left behind 2 xLP tokens, so the pending amount plus those 2
        // should always be the total xLP available.
        assert_eq!(
            Ok(new_pending),
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

#[test]
fn reinvest_partial() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Get some xLP and no LP
    let new_lp = Addr::unchecked("new-lp");
    market
        .exec_mint_and_deposit_liquidity(&new_lp, "100".parse().unwrap())
        .unwrap();

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
    let lp_info = market.query_lp_info(&new_lp).unwrap();
    assert_ne!(lp_info.available_yield, Collateral::zero());

    // Reinvest half
    let balance_before = market.query_collateral_balance(&new_lp).unwrap();
    let third = NonZero::new(
        lp_info
            .available_yield
            .div_non_zero_dec("3".parse().unwrap()),
    )
    .unwrap();
    market
        .exec_reinvest_yield(&new_lp, Some(third), false)
        .unwrap();

    let balance_after = market.query_collateral_balance(&new_lp).unwrap();
    let two_thirds = (lp_info.available_yield - third.raw()).unwrap();

    // Use approximate equals because we don't handle the "dust" (collateral below the precision of the CW20);
    let diff = (balance_after - balance_before).unwrap();
    assert!(
        diff.approx_eq_eps(two_thirds.into_number(), Number::EPS_E6)
            .unwrap(),
        "{diff} != {two_thirds}"
    );

    // Nothing left to reinvest
    market
        .exec_reinvest_yield(&new_lp, None, false)
        .unwrap_err();
}

#[test]
fn lp_transfer() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let cranker = Addr::unchecked("cranker");

    let claim_yield = |lp: &Addr| -> Number {
        let wallet_balance_before_claim = market.query_collateral_balance(lp).unwrap();
        let _ = market.exec_claim_yield(lp);
        let wallet_balance_after_claim = market.query_collateral_balance(lp).unwrap();

        (wallet_balance_after_claim - wallet_balance_before_claim).unwrap()
    };

    // Get some LP
    let lp1 = Addr::unchecked("new-lp-1");
    let lp2 = Addr::unchecked("new-lp-2");
    let amount = Number::from(100u64);

    market
        .exec_mint_and_deposit_liquidity(&lp1, amount)
        .unwrap();

    let info1 = market.query_lp_info(&lp1).unwrap();
    assert!(info1.lp_amount > LpToken::zero());
    assert_eq!(info1.xlp_amount, LpToken::zero());

    let info2 = market.query_lp_info(&lp2).unwrap();
    assert_eq!(info2.lp_amount, LpToken::zero());
    assert_eq!(info2.xlp_amount, LpToken::zero());

    // No yield to anyone initially
    assert_eq!(info1.available_yield, Collateral::zero());
    assert_eq!(info2.available_yield, Collateral::zero());

    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Lp, &lp1, &lp2, amount)
        .unwrap();

    // lp amount transferred
    let info1 = market.query_lp_info(&lp1).unwrap();
    assert_eq!(info1.lp_amount, LpToken::zero());
    assert_eq!(info1.xlp_amount, LpToken::zero());

    let info2 = market.query_lp_info(&lp2).unwrap();
    assert!(info2.lp_amount > LpToken::zero());
    assert_eq!(info2.xlp_amount, LpToken::zero());

    // Still no yield
    assert_eq!(info1.available_yield, Collateral::zero());
    assert_eq!(info2.available_yield, Collateral::zero());

    // Have a trader open a position and keep it running for a while for borrow fees
    let trader = market.clone_trader(0).unwrap();
    market
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

    market.exec_refresh_price().unwrap();
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    // Get info
    let info1 = market.query_lp_info(&lp1).unwrap();
    let info2 = market.query_lp_info(&lp2).unwrap();

    // LP1 should not have any yield
    assert_eq!(info1.available_yield, Collateral::zero());

    // But LP2 should
    assert_ne!(info2.available_yield, Collateral::zero());

    // Neither has xLP or LP yield
    assert_eq!(info1.xlp_amount, LpToken::zero());
    assert_eq!(info1.available_yield_xlp, Collateral::zero());
    assert_eq!(info2.xlp_amount, LpToken::zero());
    assert_eq!(info2.available_yield_xlp, Collateral::zero());

    // LP1 cannot claim anything
    assert_eq!(claim_yield(&lp1), Number::zero());

    // LP2 can
    assert_ne!(claim_yield(&lp2), Number::zero());

    // LP1 cannot stake
    market.exec_stake_lp(&lp1, None).unwrap_err();

    // LP2 can
    market.exec_stake_lp(&lp2, None).unwrap();

    // accummulate more fees
    market.exec_refresh_price().unwrap();
    market.set_time(TimeJump::Liquifundings(1)).unwrap();
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    // Get info
    let info1 = market.query_lp_info(&lp1).unwrap();
    let info2 = market.query_lp_info(&lp2).unwrap();

    // LP1 should not have any yield
    assert_eq!(info1.available_yield, Collateral::zero());

    // But LP2 should
    assert_ne!(info2.available_yield, Collateral::zero());

    // LP1 does not have XLP
    assert_eq!(info1.xlp_amount, LpToken::zero());
    assert_eq!(info1.available_yield_xlp, Collateral::zero());

    // LP2 does
    assert_ne!(info2.xlp_amount, LpToken::zero());
    assert_ne!(info2.available_yield_xlp, Collateral::zero());

    // LP1 cannot claim anything
    assert_eq!(claim_yield(&lp1), Number::zero());

    // LP2 can
    assert_ne!(claim_yield(&lp2), Number::zero());

    // Nothing left for LP2 to claim
    assert_eq!(claim_yield(&lp2), Number::zero());
}

#[test]
fn rewards_while_unstaking() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.automatic_time_jump_enabled = false;

    let trader = market.clone_trader(0).unwrap();
    let cranker = Addr::unchecked("cranker");
    let justlp = Addr::unchecked("justlp");
    let justxlp = Addr::unchecked("justxlp");
    let unstaking = Addr::unchecked("unstaking");

    // Have all three wallets deposit, open a position, and crank for a while to
    // get different LP vs xLP rates.
    for addr in [&justlp, &justxlp, &unstaking] {
        market
            .exec_mint_and_deposit_liquidity(addr, "1000".parse().unwrap())
            .unwrap();
    }
    for addr in [&justxlp, &unstaking] {
        market.exec_stake_lp(addr, None).unwrap();
    }

    market.automatic_time_jump_enabled = true;
    market
        .exec_open_position(
            &trader,
            "100",
            "5",
            DirectionToBase::Long,
            "2",
            None,
            None,
            None,
        )
        .unwrap();
    market.automatic_time_jump_enabled = false;

    market.set_time(TimeJump::Liquifundings(3)).unwrap();
    market.exec_refresh_price().unwrap();
    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    let status = market.query_status().unwrap();
    let per_lp_token = status.borrow_fee_lp / status.liquidity.total_lp.into_decimal256();
    let per_xlp_token = status.borrow_fee_xlp / status.liquidity.total_xlp.into_decimal256();
    assert!(
        per_lp_token < per_xlp_token,
        "Borrow fee per LP token {per_lp_token} not less than per xLP token {per_xlp_token}",
    );

    let justlp_info = market.query_lp_info(&justlp).unwrap();
    assert_eq!(justlp_info.available_yield_xlp, Collateral::zero());
    assert_ne!(justlp_info.available_yield_lp, Collateral::zero());
    let justxlp_info = market.query_lp_info(&justxlp).unwrap();
    assert_ne!(justxlp_info.available_yield_xlp, Collateral::zero());
    assert_eq!(justxlp_info.available_yield_lp, Collateral::zero());
    let unstaking_info = market.query_lp_info(&unstaking).unwrap();
    assert_eq!(
        unstaking_info.available_yield_lp,
        justxlp_info.available_yield_lp
    );
    assert_eq!(
        unstaking_info.available_yield_xlp,
        justxlp_info.available_yield_xlp
    );

    for addr in [&justlp, &justxlp, &unstaking] {
        market.exec_claim_yield(addr).unwrap();
        assert_eq!(
            market.query_lp_info(addr).unwrap().available_yield,
            Collateral::zero()
        );
    }

    // Now begin an unstaking process for the unstaking wallet and ensure the
    // unstaking wallet acts like the justlp wallet for rewards.
    market.exec_unstake_xlp(&unstaking, None).unwrap();

    market.automatic_time_jump_enabled = true;
    // Get more yields
    market
        .exec_open_position(
            &trader,
            "100",
            "5",
            DirectionToBase::Short,
            "2",
            None,
            None,
            None,
        )
        .unwrap();
    market.automatic_time_jump_enabled = false;

    market.set_time(TimeJump::Liquifundings(3)).unwrap();
    market.exec_crank_till_finished(&cranker).unwrap();

    let justlp_info = market.query_lp_info(&justlp).unwrap();
    assert_eq!(justlp_info.available_yield_xlp, Collateral::zero());
    assert_ne!(justlp_info.available_yield_lp, Collateral::zero());
    let justxlp_info = market.query_lp_info(&justxlp).unwrap();
    assert_ne!(justxlp_info.available_yield_xlp, Collateral::zero());
    assert_eq!(justxlp_info.available_yield_lp, Collateral::zero());

    let unstaking_info = market.query_lp_info(&unstaking).unwrap();
    assert_eq!(
        unstaking_info.available_yield_lp,
        justlp_info.available_yield_lp
    );
    assert_eq!(
        unstaking_info.available_yield_xlp,
        justlp_info.available_yield_xlp
    );
}

#[test]
fn transfer_while_unstaking() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.automatic_time_jump_enabled = false;

    let trader = market.clone_trader(0).unwrap();
    let unstaking = Addr::unchecked("unstaking");

    market
        .exec_mint_and_deposit_liquidity(&unstaking, "1000".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&unstaking, None).unwrap();
    market.exec_unstake_xlp(&unstaking, None).unwrap();

    // Transfering some xLP should be impossible
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Xlp,
            &unstaking,
            &trader,
            "0.001".parse().unwrap(),
        )
        .unwrap_err();

    // Transfer LP should also be impossible right now since we haven't unstaked anything yet
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Lp,
            &unstaking,
            &trader,
            "0.001".parse().unwrap(),
        )
        .unwrap_err();

    // Wait some blocks
    market.set_time(TimeJump::Hours(24)).unwrap();

    // Now transferring some LP should be possible
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Lp,
            &unstaking,
            &trader,
            "1".parse().unwrap(),
        )
        .unwrap();

    // But still no xLP
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Xlp,
            &unstaking,
            &trader,
            "0.001".parse().unwrap(),
        )
        .unwrap_err();

    // Now wait the rest of the period and transfer the remaining LP
    market.set_time(TimeJump::Hours(24 * 20)).unwrap();
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Lp,
            &unstaking,
            &trader,
            "999".parse().unwrap(),
        )
        .unwrap();
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Lp,
            &unstaking,
            &trader,
            "0.00001".parse().unwrap(),
        )
        .unwrap_err();
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Xlp,
            &unstaking,
            &trader,
            "0.001".parse().unwrap(),
        )
        .unwrap_err();
}

#[test]
fn xlp_balance_is_transferable() {
    // We want to ensure that however many xLP tokens the liquidity token
    // contract says we have, we're able to transfer that amount.
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.automatic_time_jump_enabled = false;

    let trader = market.clone_trader(0).unwrap();
    let unstaking = Addr::unchecked("unstaking");

    market
        .exec_mint_and_deposit_liquidity(&unstaking, "1000".parse().unwrap())
        .unwrap();
    market.exec_stake_lp(&unstaking, None).unwrap();
    market
        .exec_unstake_xlp(&unstaking, Some("500".parse().unwrap()))
        .unwrap();

    assert_eq!(
        market
            .query_liquidity_token_balance_raw(LiquidityTokenKind::Xlp, &unstaking)
            .unwrap(),
        Uint128::new(500_000_000)
    );
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Xlp,
            &unstaking,
            &trader,
            "500".parse().unwrap(),
        )
        .unwrap();

    assert_eq!(
        market
            .query_liquidity_token_balance_raw(LiquidityTokenKind::Xlp, &unstaking)
            .unwrap(),
        Uint128::new(0)
    );
    market
        .exec_liquidity_token_transfer(
            LiquidityTokenKind::Xlp,
            &unstaking,
            &trader,
            "0.0001".parse().unwrap(),
        )
        .unwrap_err();
}

#[test]
fn lp_yield_perp_1023() {
    let market = PerpsMarket::lp_prep(PerpsApp::new_cell().unwrap()).unwrap();
    let lp_yield = LpYield {
        pos_collateral: "812.965262".parse().unwrap(),
        pos_direction: DirectionToBase::Long,
        lp_deposit: "23.39196".parse().unwrap(),
        time_jump_liquifundings: 8.999921109112767,
        close_position: false,
        market: Rc::new(RefCell::new(market)),
    };

    lp_yield.run().unwrap();
}

#[test]
fn max_liquidity() {
    let app = PerpsApp::new_cell().unwrap();
    let token_init = match DEFAULT_MARKET.token_kind {
        TokenKind::Native => TokenInit::Native {
            denom: TEST_CONFIG.native_denom.to_string(),
            decimal_places: 6,
        },
        TokenKind::Cw20 => {
            let addr = app
                .borrow_mut()
                .get_cw20_addr(&DEFAULT_MARKET.cw20_symbol)
                .unwrap();
            TokenInit::Cw20 { addr: addr.into() }
        }
    };
    let market = PerpsMarket::new_custom(
        app,
        MarketId::new(
            "ETH".to_owned(),
            "USDC".to_owned(),
            DEFAULT_MARKET.collateral_type,
        ),
        token_init,
        "1".parse().unwrap(),
        Some("1".parse().unwrap()),
        None,
        false,
        DEFAULT_MARKET.spot_price,
    )
    .unwrap();
    let lp = market.clone_lp(0).unwrap();
    let trader = market.clone_trader(0).unwrap();

    market
        .exec_set_config(ConfigUpdate {
            max_liquidity: Some(msg::contracts::market::config::MaxLiquidity::Usd {
                amount: "1000".parse().unwrap(),
            }),
            ..Default::default()
        })
        .unwrap();
    market
        .exec_set_price_with_usd("1".parse().unwrap(), Some("1".parse().unwrap()))
        .unwrap();

    market
        .exec_mint_and_deposit_liquidity(&lp, "1000".parse().unwrap())
        .unwrap();
    let err = market
        .exec_mint_and_deposit_liquidity(&lp, "1".parse().unwrap())
        .unwrap_err()
        .downcast::<PerpError<MarketError>>()
        .unwrap();
    assert_eq!(err.id, ErrorId::MaxLiquidity);

    market
        .exec_set_config(ConfigUpdate {
            max_liquidity: Some(msg::contracts::market::config::MaxLiquidity::Usd {
                amount: "2000".parse().unwrap(),
            }),
            ..Default::default()
        })
        .unwrap();
    market
        .exec_mint_and_deposit_liquidity(&lp, "1001".parse().unwrap())
        .unwrap_err();
    market
        .exec_mint_and_deposit_liquidity(&lp, "1000".parse().unwrap())
        .unwrap();

    // Take out some liquidity and then cause impairment to push the value above the 2000 limit again
    market
        .exec_withdraw_liquidity(&lp, Some("1".parse().unwrap()))
        .unwrap();
    let (pos_id, _) = market
        .exec_open_position(
            &trader,
            "5",
            "10",
            DirectionToBase::Short,
            "2",
            None,
            None,
            None,
        )
        .unwrap();
    market
        .exec_set_price_with_usd("50".parse().unwrap(), Some("1".parse().unwrap()))
        .unwrap();
    market.exec_crank_till_finished(&trader).unwrap();
    let closed = market.query_closed_position(&trader, pos_id).unwrap();
    assert_eq!(
        closed.reason,
        PositionCloseReason::Liquidated(LiquidationReason::Liquidated)
    );
    market
        .exec_set_price_with_usd("1".parse().unwrap(), Some("1".parse().unwrap()))
        .unwrap();
    market.exec_crank_till_finished(&lp).unwrap();
    market
        .exec_mint_and_deposit_liquidity(&lp, "1".parse().unwrap())
        .unwrap_err();

    // Now drop the price of collateral in terms of USD so that we're back under the limit
    market
        .exec_set_price_with_usd("1".parse().unwrap(), Some("0.1".parse().unwrap()))
        .unwrap();
    market.exec_crank_till_finished(&lp).unwrap();
    market
        .exec_mint_and_deposit_liquidity(&lp, "1".parse().unwrap())
        .unwrap();

    // Set the price back and we can't deposit again
    market
        .exec_set_price_with_usd("1".parse().unwrap(), Some("1".parse().unwrap()))
        .unwrap();
    market.exec_crank_till_finished(&lp).unwrap();
    market
        .exec_mint_and_deposit_liquidity(&lp, "1".parse().unwrap())
        .unwrap_err();
}

#[test]
fn reinvest_history_perp_1418() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Get some LP
    let new_lp = Addr::unchecked("new-lp");
    market
        .exec_mint_and_deposit_liquidity(&new_lp, "100".parse().unwrap())
        .unwrap();

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

    for _ in 0..10 {
        market.set_time(TimeJump::Hours(6)).unwrap();
        market.exec_refresh_price().unwrap();
        market.exec_crank_till_finished(&trader).unwrap();
    }

    // Should have some yield
    let lp_info = market.query_lp_info(&new_lp).unwrap();
    assert_ne!(lp_info.available_yield, Collateral::zero());

    // Get our current history events
    let events_before_reinvest = market.query_lp_action_history(&new_lp).unwrap();

    // Reinvest
    market.exec_reinvest_yield(&new_lp, None, false).unwrap();

    // Get new events, make sure there's only one more
    let events_after_reinvest = market.query_lp_action_history(&new_lp).unwrap();
    assert_eq!(
        events_before_reinvest.actions.len() + 1,
        events_after_reinvest.actions.len()
    );
}

#[test]
fn reinvest_history_perp_1418_partial() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    // Get some LP
    let new_lp = Addr::unchecked("new-lp");
    market
        .exec_mint_and_deposit_liquidity(&new_lp, "100".parse().unwrap())
        .unwrap();

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

    for _ in 0..10 {
        market.set_time(TimeJump::Hours(6)).unwrap();
        market.exec_refresh_price().unwrap();
        market.exec_crank_till_finished(&trader).unwrap();
    }

    // Should have some yield
    let lp_info = market.query_lp_info(&new_lp).unwrap();
    assert_ne!(lp_info.available_yield, Collateral::zero());

    // Get our current history events
    let events_before_reinvest = market.query_lp_action_history(&new_lp).unwrap();

    // Reinvest
    market
        .exec_reinvest_yield(
            &new_lp,
            Some(
                NonZero::new(
                    lp_info
                        .available_yield
                        .checked_mul_dec("0.5".parse().unwrap())
                        .unwrap(),
                )
                .unwrap(),
            ),
            false,
        )
        .unwrap();

    // Get new events, make sure there are two: one for the reinvest, one for the claim
    let events_after_reinvest = market.query_lp_action_history(&new_lp).unwrap();
    assert_eq!(
        events_before_reinvest.actions.len() + 2,
        events_after_reinvest.actions.len()
    );
}

#[derive(Debug)]
struct OpenParam {
    collateral: Number,
    leverage: LeverageToBase,
    max_gains: MaxGainsInQuote,
}

#[test]
fn carry_leverage_min_liquidity_open() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let open_param_success = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "2307".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "4285".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
    };

    let (pos_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param_success.collateral,
            None,
            open_param_success.leverage,
            DirectionToBase::Long,
            open_param_success.max_gains,
            None,
            None,
        )
        .unwrap();

    market.exec_close_position(&trader, pos_id, None).unwrap();

    let open_param_fail = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "2308".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "4286".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
    };

    let response = market.exec_open_position_raw(
        &trader,
        open_param_fail.collateral,
        None,
        open_param_fail.leverage,
        DirectionToBase::Long,
        open_param_fail.max_gains,
        None,
        None,
    );
    assert!(response.is_err());

    market
        .exec_mint_and_deposit_liquidity(&trader, "2000".parse().unwrap())
        .unwrap();

    market
        .exec_open_position_raw(
            &trader,
            open_param_fail.collateral,
            None,
            open_param_fail.leverage,
            DirectionToBase::Long,
            open_param_fail.max_gains,
            None,
            None,
        )
        .unwrap();
}

#[test]
fn carry_leverage_min_liquidity_update_position_size() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let open_param_success = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "2306".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "4284".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
    };

    let (pos_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param_success.collateral,
            None,
            open_param_success.leverage,
            DirectionToBase::Long,
            open_param_success.max_gains,
            None,
            None,
        )
        .unwrap();

    market
        .exec_update_position_collateral_impact_size(&trader, pos_id, "1".try_into().unwrap(), None)
        .unwrap();

    let response = market.exec_update_position_collateral_impact_size(
        &trader,
        pos_id,
        "1".try_into().unwrap(),
        None,
    );

    assert!(response.is_err());
}

#[test]
fn carry_leverage_min_liquidity_update_leverage() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let open_param_success = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "2000".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "4000".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
    };

    let (pos_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param_success.collateral,
            None,
            open_param_success.leverage,
            DirectionToBase::Long,
            open_param_success.max_gains,
            None,
            None,
        )
        .unwrap();

    market
        .exec_update_position_leverage(&trader, pos_id, "3.02".parse().unwrap(), None)
        .unwrap();

    let response =
        market.exec_update_position_leverage(&trader, pos_id, "3.5".parse().unwrap(), None);
    assert!(response.is_err());
}

#[test]
fn carry_leverage_min_liquidity_update_max_gains() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    market
        .exec_set_config(ConfigUpdate {
            trading_fee_notional_size: Some("0".parse().unwrap()),
            trading_fee_counter_collateral: Some("0".parse().unwrap()),
            delta_neutrality_fee_sensitivity: Some("5000000000000000000".parse().unwrap()),
            ..Default::default()
        })
        .unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let open_param_success = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "2000".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "4000".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
    };

    let (pos_id, _) = market
        .exec_open_position_raw(
            &trader,
            open_param_success.collateral,
            None,
            open_param_success.leverage,
            DirectionToBase::Long,
            open_param_success.max_gains,
            None,
            None,
        )
        .unwrap();

    market
        .exec_update_position_max_gains(&trader, pos_id, "1.02".parse().unwrap())
        .unwrap();
    let response = market.exec_update_position_max_gains(&trader, pos_id, "1.5".parse().unwrap());
    assert!(response.is_err());
}

#[test]
fn carry_leverage_min_liquidity_withdraw() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let new_lp = Addr::unchecked("new-lp");
    market
        .exec_mint_and_deposit_liquidity(&new_lp, "3000".parse().unwrap())
        .unwrap();

    let market_type = market.id.get_market_type();
    let trader = market.clone_trader(0).unwrap();

    let open_param_success = match market_type {
        MarketType::CollateralIsQuote => OpenParam {
            collateral: "3460".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
        MarketType::CollateralIsBase => OpenParam {
            collateral: "6427".parse().unwrap(),
            leverage: "3".parse().unwrap(),
            max_gains: "1".parse().unwrap(),
        },
    };

    market
        .exec_open_position_raw(
            &trader,
            open_param_success.collateral,
            None,
            open_param_success.leverage,
            DirectionToBase::Long,
            open_param_success.max_gains,
            None,
            None,
        )
        .unwrap();

    let withdraw_amount_fail = Number::from(1505u64);
    let response = market.exec_withdraw_liquidity(&new_lp, Some(withdraw_amount_fail));
    assert!(response.is_err());

    let withdraw_amount_success = Number::from(1500u64);
    market
        .exec_withdraw_liquidity(&new_lp, Some(withdraw_amount_success))
        .unwrap();
}

#[test]
fn liquidity_cooldown_works() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.automatic_time_jump_enabled = false;
    market
        .exec_set_config(ConfigUpdate {
            liquidity_cooldown_seconds: Some(3600),
            ..Default::default()
        })
        .unwrap();

    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();

    // Create an LP that is not in the cooldown period by waiting past the
    // 3600 seconds.
    market
        .exec_mint_and_deposit_liquidity(&lp0, "1000".parse().unwrap())
        .unwrap();
    market.set_time(TimeJump::Hours(1)).unwrap();

    // Now start a new deposit for lp1 and check that withdrawing and transferring LP tokens fails.
    market
        .exec_mint_and_deposit_liquidity(&lp1, "1000".parse().unwrap())
        .unwrap();
    let err = market
        .exec_withdraw_liquidity(&lp1, Some("1".parse().unwrap()))
        .unwrap_err();
    let err = MarketError::try_from_anyhow(&err).unwrap();
    match err {
        MarketError::LiquidityCooldown {
            ends_at: _,
            seconds_remaining,
        } => assert_eq!(seconds_remaining, 3600),
        _ => Err(err).unwrap(),
    }
    let err = market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Lp, &lp1, &lp0, "1".parse().unwrap())
        .unwrap_err();
    let err = MarketError::try_from_anyhow(&err).unwrap();
    match err {
        MarketError::LiquidityCooldown {
            ends_at: _,
            seconds_remaining,
        } => assert_eq!(seconds_remaining, 3600),
        _ => Err(err).unwrap(),
    }

    // But it's fine for the original LP to transfer to us
    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Lp, &lp0, &lp1, "1".parse().unwrap())
        .unwrap();

    // It's also OK to stake LP into xLP, and to transfer that xLP
    market
        .exec_stake_lp(&lp1, Some("500".parse().unwrap()))
        .unwrap();
    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Xlp, &lp1, &lp0, "1".parse().unwrap())
        .unwrap();

    // Wait an hour and everything should work
    market.set_time(TimeJump::Hours(1)).unwrap();
    market
        .exec_withdraw_liquidity(&lp1, Some("1".parse().unwrap()))
        .unwrap();
    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Lp, &lp1, &lp0, "1".parse().unwrap())
        .unwrap();
    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Xlp, &lp1, &lp0, "1".parse().unwrap())
        .unwrap();
}
#[test]
fn liquidity_cooldown_no_cooldown_xlp() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    market.automatic_time_jump_enabled = false;
    market
        .exec_set_config(ConfigUpdate {
            liquidity_cooldown_seconds: Some(3600),
            ..Default::default()
        })
        .unwrap();

    let lp0 = market.clone_lp(0).unwrap();
    let lp1 = market.clone_lp(1).unwrap();

    // Do some deposits for both and then wait an hour
    market
        .exec_mint_and_deposit_liquidity(&lp0, "1000".parse().unwrap())
        .unwrap();
    market
        .exec_mint_and_deposit_liquidity(&lp1, "1000".parse().unwrap())
        .unwrap();
    market.set_time(TimeJump::Hours(1)).unwrap();

    // Have lp0 deposit into LP, and lp1 deposit into xLP.

    // Now start a new deposit for lp1 and check that withdrawing and transferring LP tokens fails.
    market
        .exec_mint_and_deposit_liquidity(&lp0, "1000".parse().unwrap())
        .unwrap();
    market
        .exec_mint_and_deposit_liquidity_xlp(&lp1, "1000".parse().unwrap())
        .unwrap();

    // lp1 can withdraw and transfer all tokens
    market
        .exec_withdraw_liquidity(&lp1, Some("1".parse().unwrap()))
        .unwrap();
    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Lp, &lp1, &lp0, "1".parse().unwrap())
        .unwrap();
    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Xlp, &lp1, &lp0, "1".parse().unwrap())
        .unwrap();

    // lp0 cannot withdraw or transfer LP, but can transfer xLP
    let err = market
        .exec_withdraw_liquidity(&lp0, Some("1".parse().unwrap()))
        .unwrap_err();
    let err = MarketError::try_from_anyhow(&err).unwrap();
    match err {
        MarketError::LiquidityCooldown {
            ends_at: _,
            seconds_remaining,
        } => assert_eq!(seconds_remaining, 3600),
        _ => Err(err).unwrap(),
    }
    let err = market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Lp, &lp0, &lp1, "1".parse().unwrap())
        .unwrap_err();
    let err = MarketError::try_from_anyhow(&err).unwrap();
    match err {
        MarketError::LiquidityCooldown {
            ends_at: _,
            seconds_remaining,
        } => assert_eq!(seconds_remaining, 3600),
        _ => Err(err).unwrap(),
    }
    market
        .exec_liquidity_token_transfer(LiquidityTokenKind::Xlp, &lp0, &lp1, "1".parse().unwrap())
        .unwrap();
}

#[test]
fn liquidity_zero_dust_2487() {
    liquidity_zero_dust_2487_inner("1.030489".parse().unwrap(), "1004.9995".parse().unwrap());
    liquidity_zero_dust_2487_inner("0.9".parse().unwrap(), "1004.9995".parse().unwrap());
}

fn liquidity_zero_dust_2487_inner(price_change_multiplier: Decimal256, lp_amount: Decimal256) {
    let market = PerpsMarket::new_with_type(
        PerpsApp::new_cell().unwrap(),
        DEFAULT_MARKET.collateral_type,
        // make sure LP starts with 0
        false,
        SpotPriceKind::Oracle,
    )
    .unwrap();

    // just get an initial price in there
    market
        .exec_crank_n(&Addr::unchecked("init-cranker"), 1)
        .unwrap();
    market.exec_refresh_price().unwrap();

    let lp = market.clone_lp(0).unwrap();
    let trader = market.clone_trader(0).unwrap();

    // sanity check that we're starting from a baseline
    let liquidity = market.query_liquidity_stats().unwrap();
    assert_eq!(liquidity.total_collateral(), Ok(Collateral::zero()));
    assert_eq!(liquidity.total_tokens(), Ok(LpToken::zero()));

    // LP Mint & Deposit
    let lp_deposit = Number::try_from(lp_amount.to_string()).unwrap();
    market.exec_mint_tokens(&lp, lp_deposit).unwrap();
    market.exec_deposit_liquidity(&lp, lp_deposit).unwrap();

    let liquidity_before_open = market.query_liquidity_stats().unwrap();

    // open a position
    let (pos_id, _) = market
        .exec_open_position_refresh_price(
            &trader,
            "100",
            "9",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();

    let counter_collateral = market.query_position(pos_id).unwrap().counter_collateral;

    // update price
    let price = market
        .query_current_price()
        .unwrap()
        .price_base
        .into_number()
        .abs_unsigned()
        .mul(price_change_multiplier);

    market
        .exec_set_price(price.to_string().parse().unwrap())
        .unwrap();
    market.set_time(TimeJump::Blocks(1)).unwrap();
    market.exec_refresh_price().unwrap();
    market.exec_crank_till_finished(&trader).unwrap();

    // close the position if it hasn't already
    if market.query_position(pos_id).is_ok() {
        market
            .exec_close_position_refresh_price(&trader, pos_id, None)
            .unwrap();
    }

    // sanity check, liquidity has gained some interesting amount
    let liquidity = market.query_liquidity_stats().unwrap();
    assert_ne!(
        Ok(liquidity.total_collateral().unwrap()),
        liquidity_before_open.total_collateral().unwrap() - counter_collateral.raw()
    );

    // withdraw all liquidity
    market.exec_withdraw_liquidity(&lp, None).unwrap();

    let liquidity = market.query_liquidity_stats().unwrap();

    // liquidity is truly zero
    assert_eq!(liquidity.total_tokens(), Ok(LpToken::zero()));
    assert_eq!(liquidity.total_collateral(), Ok(Collateral::zero()));

    // demonstrate that the protocol is still working fine
    let lp_deposit = Number::from(1000u64);
    market.exec_mint_tokens(&lp, lp_deposit).unwrap();
    market.exec_deposit_liquidity(&lp, lp_deposit).unwrap();

    // open a position
    market
        .exec_open_position_refresh_price(
            &trader,
            "100",
            "9",
            DirectionToBase::Long,
            "1.0",
            None,
            None,
            None,
        )
        .unwrap();
}

proptest! {
    // run this to get some error conditions for the explicit test above
    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_liquidity_zero_dust_2487(
        price_change_multiplier in 0.9f32..1.1f32,
        lp_amount in 1000.0f32..1010.0f32
    )
    {
        liquidity_zero_dust_2487_inner(
            price_change_multiplier.to_string().parse().unwrap(),
            lp_amount.to_string().parse().unwrap()
        );
    }
}
