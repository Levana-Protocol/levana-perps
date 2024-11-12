use crate::{extensions::TokenExt, time::TimeJump};

use super::data::*;
use anyhow::Result;
use cosmwasm_std::testing::MockApi;
use perpswap::prelude::*;

impl LpDepositWithdraw {
    pub fn run(&self) -> Result<()> {
        let market = self.market.borrow_mut();
        let mock_api = MockApi::default();

        market.exec_refresh_price()?;
        market.exec_crank_till_finished(&mock_api.addr_make("cranker"))?;

        let init_liquidity_stats = market.query_liquidity_stats().unwrap();
        let collateral = self.collateral.into_number();
        let deposit = self.deposit.into_number();
        let withdraw = self.withdraw.into_number();
        let lp = mock_api.addr_make("new-lp");

        // mint some tokens so we _can_ deposit
        market
            .exec_mint_tokens(&lp, self.collateral.into_number())
            .unwrap();

        let balance_before_deposit = market.query_collateral_balance(&lp).unwrap();

        // deposit LP
        market.exec_deposit_liquidity(&lp, deposit).unwrap();

        let liquidity_stats_after_deposit = market.query_liquidity_stats().unwrap();
        let balance_after_deposit = market.query_collateral_balance(&lp).unwrap();

        // jump time
        market.set_time(self.time_jump)?;
        market.exec_refresh_price()?;

        // Withdraw
        market.exec_withdraw_liquidity(&lp, Some(withdraw)).unwrap();

        let liquidity_stats_after_withdraw = market.query_liquidity_stats().unwrap();

        let balance_after_withdraw = market.query_collateral_balance(&lp).unwrap();

        // Assert
        assert_eq!(balance_before_deposit, collateral);
        assert_eq!(balance_after_deposit, (collateral - deposit).unwrap());
        assert_eq!(
            (liquidity_stats_after_deposit
                .total_collateral()
                .unwrap()
                .into_number()
                - init_liquidity_stats
                    .total_collateral()
                    .unwrap()
                    .into_number())
            .unwrap(),
            deposit
        );
        assert_eq!(
            liquidity_stats_after_withdraw
                .total_collateral()
                .unwrap()
                .into_number(),
            ((init_liquidity_stats
                .total_collateral()
                .unwrap()
                .into_number()
                + deposit)
                .unwrap()
                - withdraw)
                .unwrap()
        );
        assert_eq!(
            balance_after_withdraw,
            ((collateral - deposit).unwrap() + withdraw).unwrap()
        );

        Ok(())
    }
}

impl XlpStakeUnstake {
    pub fn run(&self) -> Result<()> {
        let market = self.market.borrow_mut();
        let config = market.query_config().unwrap();
        let mock_api = MockApi::default();

        market.exec_refresh_price()?;
        market.exec_crank_till_finished(&mock_api.addr_make("cranker"))?;

        let deposit = self.deposit.into_number();
        let stake = self.stake.into_number();
        let unstake = self.unstake.into_number();

        let lp = mock_api.addr_make("new-lp");

        // deposit LP
        market
            .exec_mint_and_deposit_liquidity(&lp, deposit)
            .unwrap();

        // try to stake more than we have - should fail
        market
            .exec_stake_lp(&lp, Some((deposit + Number::ONE).unwrap()))
            .unwrap_err();

        // stake should succeed
        market.exec_stake_lp(&lp, Some(stake)).unwrap();
        let expected_stake_lp_amount = (deposit - stake).unwrap();
        let expected_stake_xlp_amount = stake;

        let info = market.query_lp_info(&lp).unwrap();
        assert_eq!(info.lp_amount.into_number(), expected_stake_lp_amount);
        assert_eq!(info.xlp_amount.into_number(), expected_stake_xlp_amount);

        // unstake more than we have staked - should fail
        market
            .exec_unstake_xlp(&lp, Some((stake + Number::ONE).unwrap()))
            .unwrap_err();

        // unstake should succeed
        market.exec_unstake_xlp(&lp, Some(unstake)).unwrap();
        let expected_unstake_lp_amount = ((deposit - stake).unwrap() + unstake).unwrap();
        let expected_unstake_xlp_amount = (stake - unstake).unwrap();

        // jump to half the unstaking period for imprecise checks
        market
            .set_time(TimeJump::Seconds(config.unstake_period_seconds as i64 / 2))
            .unwrap();
        let info = market.query_lp_info(&lp).unwrap();
        assert!(
            info.lp_amount.into_number() > expected_stake_lp_amount
                && info.lp_amount.into_number() < expected_unstake_lp_amount
        );
        assert!(
            info.xlp_amount.into_number() < expected_stake_xlp_amount
                && info.xlp_amount.into_number() > expected_unstake_xlp_amount
        );

        // finish the unstaking period for precise checks
        market
            .set_time(TimeJump::Seconds(
                (config.unstake_period_seconds as i64 / 2) + 1,
            ))
            .unwrap();
        let info = market.query_lp_info(&lp).unwrap();
        assert_eq!(info.lp_amount.into_number(), expected_unstake_lp_amount);
        assert_eq!(info.xlp_amount.into_number(), expected_unstake_xlp_amount);

        // even though we're unstaked, we must collect in order for it to be recognized
        assert!(info.unstaking.is_some());

        // Now collect everything
        market.exec_collect_unstaked_lp(&lp).unwrap();
        let info = market.query_lp_info(&lp).unwrap();
        assert_eq!(info.unstaking, None);

        // the amounts are unchanged, i.e. we've still just unstaked
        assert_eq!(info.lp_amount.into_number(), expected_unstake_lp_amount);
        assert_eq!(info.xlp_amount.into_number(), expected_unstake_xlp_amount);

        // And collecting a second time should fail
        market.exec_collect_unstaked_lp(&lp).unwrap_err();

        Ok(())
    }
}

impl LpYield {
    pub fn run(&self) -> Result<()> {
        let market = &mut *self.market.borrow_mut();
        let market_config = market.query_config().unwrap();
        let mock_api = MockApi::default();
        let lp = mock_api.addr_make("new-lp");
        let init_lp_pool = (self.pos_collateral.into_number() * market_config.max_leverage)?;
        let lp_deposit = self.lp_deposit.into_number();
        let trader = market.clone_trader(0).unwrap();
        //market.automatic_time_jump_enabled = false;

        // we're starting with a market without any LP, so create the pool
        // from a different user, with enough to support trades
        market
            .exec_mint_and_deposit_liquidity(&mock_api.addr_make("prior-lp-pool"), init_lp_pool)
            .unwrap();

        // deposit our liquidity
        market
            .exec_mint_and_deposit_liquidity(&lp, lp_deposit)
            .unwrap();

        // // Open position

        market.exec_refresh_price()?;
        let cranker = mock_api.addr_make("cranker");
        market.exec_crank_till_finished(&cranker)?;

        let (pos_id, _) = market
            .exec_open_position(
                &trader,
                self.pos_collateral.into_number(),
                "10",
                self.pos_direction,
                "1",
                None,
                None,
                None,
            )
            .unwrap();

        // allow enough time to accrue yield
        market
            .set_time(TimeJump::FractionalLiquifundings(
                self.time_jump_liquifundings,
            ))
            .unwrap();
        market.exec_refresh_price().unwrap();
        market.exec_crank_till_finished(&cranker).unwrap();

        if self.close_position {
            market.exec_close_position(&trader, pos_id, None).unwrap();
        }

        let expected_yield = {
            let (trading_fee, borrow_fee) = if self.close_position {
                let pos = market.query_closed_position(&trader, pos_id).unwrap();

                // it would be nice to manually confirm these values like we do with open positions
                // but we're missing `notional_size_in_collateral` and `counter_collateral` in closed positions
                // in theory we could add these to the closed position struct, but it's not worth it
                // since these values change over time and where it happens to be at the time of close
                // doesn't accurately reflect the fees that were accumulated
                (
                    pos.trading_fee_collateral.into_number(),
                    pos.borrow_fee_collateral.into_number(),
                )
            } else {
                let pos = market.query_position(pos_id).unwrap();

                // Calculate trading fee
                let mut trading_fee =
                    (pos.notional_size_in_collateral.abs_unsigned().into_number()
                        * market_config.trading_fee_notional_size.into_number())?;
                trading_fee = (trading_fee
                    + (pos.counter_collateral.into_number()
                        * market_config.trading_fee_counter_collateral.into_number())?)?;

                assert_eq!(pos.trading_fee_collateral.into_number(), trading_fee);

                (trading_fee, pos.borrow_fee_collateral.into_number())
            };

            // the total available yield is the sum of the trading fee and the borrow fee, minus the protocol tax
            let total_yield = ((borrow_fee + trading_fee)?.into_number()
                * (Number::ONE - market_config.protocol_tax.into_number())?)?;

            // our yield is the total yield, relative to our portion of the pool
            let our_pool_portion = (lp_deposit / (lp_deposit + init_lp_pool)?)?;
            let our_yield = (total_yield * our_pool_portion)?;

            // sanity check
            let global_lp_stats = market.query_liquidity_stats().unwrap();
            let our_lp_stats = market.query_lp_info(&lp).unwrap();
            assert_eq!(
                global_lp_stats.total_lp.into_number(),
                (lp_deposit + init_lp_pool)?
            );
            assert_eq!(our_lp_stats.lp_amount.into_number(), lp_deposit);

            our_yield
        };

        let actual_yield = {
            let wallet_balance_before_claim = market.query_collateral_balance(&lp).unwrap();
            market.exec_claim_yield(&lp).unwrap();
            let wallet_balance_after_claim = market.query_collateral_balance(&lp).unwrap();

            wallet_balance_after_claim - wallet_balance_before_claim
        }
        .unwrap();

        market.token.assert_eq_signed(actual_yield, expected_yield);

        Ok(())
    }
}
