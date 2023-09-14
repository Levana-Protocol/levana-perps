use crate::{
    arbitrary::farming::emissions::data::{Action, ActionKind},
    config::TEST_CONFIG,
    extensions::TokenExt,
    time::TimeJump,
};

use super::data::FarmingEmissions;
use anyhow::Result;
use msg::prelude::*;

impl FarmingEmissions {
    pub fn run(&self) -> Result<()> {
        let lp = self.market.borrow().clone_lp(0).unwrap();

        self.do_simulation(&lp)?;

        let claimed = self.claim_lvn_rewards(&lp)?;
        self.check_actions_claim(&lp, claimed);

        self.check_last_remaining_rewards(&lp)?;

        Ok(())
    }

    fn check_last_remaining_rewards(&self, lp: &Addr) -> Result<()> {
        if let Some(last_action) = self.actions.last() {
            println!("checking last remaining rewards");
            let market = self.market.borrow_mut();
            let last_action_time = last_action.at_seconds;
            let remaining_emissions_seconds = self.emissions_duration_seconds - last_action_time;
            let token = market.rewards_token();

            market
                .set_time(TimeJump::Seconds(remaining_emissions_seconds.into()))
                .unwrap();

            let balance_before_claim = market.query_reward_token_balance(&token, lp);
            let farmer_stats_before_claim = market.query_farming_farmer_stats(lp).unwrap();

            if farmer_stats_before_claim.emission_rewards == LvnToken::zero() {
                println!("No LVN rewards to check");
                return Ok(());
            }

            market.exec_farming_claim_emissions(lp).unwrap();

            let balance_after_claim = market.query_reward_token_balance(&token, lp);

            token.assert_eq(
                NonZero::new(farmer_stats_before_claim.emission_rewards).unwrap(),
                NonZero::new(balance_after_claim - balance_before_claim).unwrap(),
            );
        } else {
            println!("no last remaining rewards");
        }

        Ok(())
    }

    fn claim_lvn_rewards(&self, lp: &Addr) -> Result<LvnToken> {
        let mut market = self.market.borrow_mut();
        market.automatic_time_jump_enabled = false;

        let token = market.rewards_token();

        let wallet_rewards_before_claim = market.query_reward_token_balance(&token, lp);
        let farmer_stats_before_claim = market.query_farming_farmer_stats(lp).unwrap();

        if farmer_stats_before_claim.emission_rewards == LvnToken::zero() {
            println!("No LVN rewards to check");
            return Ok(LvnToken::zero());
        }
        market.exec_farming_claim_emissions(lp).unwrap();

        let wallet_rewards_after_claim = market.query_reward_token_balance(&token, lp);
        let wallet_rewards_claimed = wallet_rewards_after_claim - wallet_rewards_before_claim;
        println!("claimed {} rewards", wallet_rewards_claimed);

        token.assert_eq(
            NonZero::new(farmer_stats_before_claim.emission_rewards).unwrap(),
            NonZero::new(wallet_rewards_claimed).unwrap(),
        );

        Ok(wallet_rewards_claimed)
    }

    fn check_actions_claim(&self, lp: &Addr, wallet_rewards_claimed: LvnToken) {
        let market = self.market.borrow_mut();
        let token = market.rewards_token();
        let expected = self.expected_rewards(lp);

        if expected > LvnToken::zero() && wallet_rewards_claimed > LvnToken::zero() {
            token.assert_eq(
                NonZero::new(expected).unwrap(),
                NonZero::new(wallet_rewards_claimed).unwrap(),
            );
        }
    }

    fn expected_rewards(&self, _lp: &Addr) -> LvnToken {
        let total_emissions_duration =
            Number::from_ratio_u256(self.emissions_duration_seconds, 1u32);
        let total_emissions_amount = self.emissions_amount.into_number();

        let mut prev_action: Option<&Action> = None;
        let mut farming_tokens_total = Number::ZERO;
        let mut accrued_emissions = Number::ZERO;

        for action in self.actions.iter() {
            let time_since_last_action = match prev_action {
                Some(prev_action) => {
                    Number::from_ratio_u256(action.at_seconds - prev_action.at_seconds, 1u32)
                }
                None => Number::from_ratio_u256(action.at_seconds, 1u32),
            };

            if farming_tokens_total > Number::ZERO {
                let lvn_per_token = total_emissions_amount / farming_tokens_total;
                let timeslice_ratio = time_since_last_action / total_emissions_duration;
                let timeslice_emissions = lvn_per_token * timeslice_ratio;
                let lp_farming_tokens = farming_tokens_total; // this will change with multiple lps
                let lp_timeslice_emissions = timeslice_emissions * lp_farming_tokens;

                accrued_emissions += lp_timeslice_emissions;
            }

            match action.kind {
                ActionKind::Deposit(collateral) => {
                    farming_tokens_total += collateral.into_number();
                }
                ActionKind::Withdraw(collateral) => {
                    farming_tokens_total -= collateral.into_number();
                }
            }

            prev_action = Some(action);
        }

        LvnToken::from_decimal256(accrued_emissions.abs_unsigned())
    }

    fn do_simulation(&self, lp: &Addr) -> Result<()> {
        println!("start simulation");
        let mut market = self.market.borrow_mut();
        market.automatic_time_jump_enabled = false;

        market.exec_farming_start_lockdrop(None).unwrap();
        market.set_time(TimeJump::Hours(24 * 365)).unwrap();
        market.exec_farming_start_launch().unwrap();

        let token = market.mint_lvn_rewards(&self.emissions_amount.to_string(), None);

        // sanity check
        let protocol_owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
        let balance = market.query_reward_token_balance(&token, &protocol_owner);
        assert_eq!(balance, self.emissions_amount);

        market
            .exec_farming_set_emissions(
                market.now(),
                self.emissions_duration_seconds,
                NonZero::new(self.emissions_amount).unwrap(),
                token,
            )
            .unwrap();

        // Test query farming rewards

        let mut time_jumped = 0;

        let balance_before_actions = market.query_collateral_balance(lp).unwrap();
        for action in self.actions.iter() {
            let seconds = action.at_seconds - time_jumped;
            println!("Jumping {} seconds to {}", seconds, action.at_seconds);
            market.set_time(TimeJump::Seconds(seconds.into())).unwrap();
            time_jumped += seconds;

            match action.kind {
                ActionKind::Deposit(collateral) => {
                    println!("depositing {} collateral worth of tokens", collateral);
                    // TODO - test different deposit types: xlp and lp too
                    market
                        .exec_farming_deposit_collateral(lp, NonZero::new(collateral).unwrap())
                        .unwrap();
                }
                ActionKind::Withdraw(collateral) => {
                    println!("withdrawing {} collateral worth of tokens", collateral);
                    // TODO - account for yield which changes collateral->farming token ratio
                    market
                        .exec_farming_withdraw_xlp(
                            lp,
                            Some(
                                NonZero::new(FarmingToken::from_decimal256(
                                    collateral.into_decimal256(),
                                ))
                                .unwrap(),
                            ),
                        )
                        .unwrap();
                }
            }
        }
        let balance_after_actions = market.query_collateral_balance(lp).unwrap();

        // farming doesn't affect our collateral balance since we minted as we go
        assert_eq!(balance_after_actions, balance_before_actions);

        Ok(())
    }

    // TODO: deprecate? do something with it?
    // returns the amount of collateral we got back
    fn _unfarm(&self, lp: &Addr) -> Result<Option<Number>> {
        let mut market = self.market.borrow_mut();
        market.automatic_time_jump_enabled = true;
        let balance_before = market.query_collateral_balance(lp).unwrap();
        let lp_info = market.query_lp_info(lp).unwrap();

        if lp_info.xlp_amount == LpToken::zero() {
            println!("No xlp to unstake, skipping");
            println!("{:#?}", lp_info);
            return Ok(None);
        }

        // begin the xlp unstaking process
        market.exec_unstake_xlp(lp, None).unwrap();
        // Jump to a long time in the future, so xlp finishes unstaking
        market.set_time(TimeJump::Hours(365 * 24)).unwrap();

        // convert xlp to lp (i.e. "unstake the xlp")
        market.exec_collect_unstaked_lp(lp).unwrap();

        // sanity check, shouldn't have any lp or xlp left
        let lp_info_after_unstake = market.query_lp_info(lp).unwrap();
        assert_eq!(lp_info_after_unstake.xlp_amount, LpToken::zero());
        assert_ne!(lp_info_after_unstake.lp_amount, LpToken::zero());
        assert_eq!(lp_info_after_unstake.unstaking, None);

        // withdraw all liquidity
        market.exec_withdraw_liquidity(lp, None).unwrap();

        let balance_after_unstake = market.query_collateral_balance(lp).unwrap();
        let balance_diff_after_unstake = balance_after_unstake - balance_before;

        Ok(Some(balance_diff_after_unstake))
    }
}
