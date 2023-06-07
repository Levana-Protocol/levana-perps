use crate::{arbitrary::farming::emissions::data::{Action, ActionKind}, time::TimeJump, config::TEST_CONFIG};

use super::data::FarmingEmissions;
use anyhow::Result;
use msg::prelude::*;

impl FarmingEmissions {
    pub fn run(&self) -> Result<()> {
        let lp = self.market.borrow().clone_lp(0).unwrap();

        self.do_simulation(&lp)?;

        if let Some(balance_diff) = self.unfarm(&lp)? {
            println!("balance diff after actions: {}", balance_diff);
        } else {
            if self.actions.iter().any(|a| matches!(a.kind, ActionKind::Withdraw(_))) {
                panic!("expected to unfarm some withdrawals, but didn't have any");
            }
        }

        if self.actions_farming_balance() > Number::ZERO {
            let farmer_info = self.market.borrow().query_farming_farmer_stats(&lp).unwrap();

            self.market
                .borrow()
                .exec_farming_withdraw_xlp(&lp, None) 
                .unwrap();

            let balance_diff = self.unfarm(&lp)?.expect("expected to unfarm some remaining withdrawals at the end, but didn't have any");
            println!("balance diff after remainder: {}", balance_diff);
        }

        Ok(())
    }

    fn actions_farming_balance(&self) -> Number {
        let mut farming_balance = Number::ZERO;
        for action in self.actions.iter() {
            match action.kind {
                ActionKind::Deposit(collateral) => {
                    farming_balance += collateral.into_number();
                },
                ActionKind::Withdraw(collateral) => {
                    farming_balance -= collateral.into_number();
                },
            }
        }

        farming_balance
    }

    fn emissions_duration_seconds(&self) -> u32 {
        let total_action_time_seconds = self.actions.last().unwrap().at_seconds;
        // emissions time is the full duration of the test, plus the amount for final remainder unstaking
        // remainder unstaking happens after a year, and enables the automatic timejumps, so add an extra day too
        
        total_action_time_seconds + 60 * 60 * 24 * 366
    }

    fn do_simulation(&self, lp: &Addr) -> Result<()> {
        let mut market = self.market.borrow_mut();
        market.automatic_time_jump_enabled = false;


    
        market.exec_farming_start_lockdrop(None).unwrap();
        market.set_time(TimeJump::Hours(24 * 365)).unwrap();
        market.exec_farming_start_launch().unwrap();
    
        let lvn_emissions_amount = "1000";
        let token = market.setup_lvn_rewards(lvn_emissions_amount);
    
        // // sanity check
        let protocol_owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
        let balance = market.query_reward_token_balance(&token, &protocol_owner);
        assert_eq!(balance, LvnToken::from_str(lvn_emissions_amount).unwrap());

        market
            .exec_farming_set_emissions(market.now(), self.emissions_duration_seconds(), lvn_emissions_amount.parse().unwrap(), token)
            .unwrap();
    
        // // Test query farming rewards

        let mut time_jumped = 0;

        let lp_info_before = market.query_lp_info(&lp).unwrap();
        let balance_before_actions = market.query_collateral_balance(&lp).unwrap();
        for action in self.actions.iter() {
            let seconds = action.at_seconds - time_jumped;
            println!("Jumping {} seconds to {}", seconds, action.at_seconds);
            market.set_time(TimeJump::Seconds(seconds.into())).unwrap();
            time_jumped += seconds;


            match action.kind {
                ActionKind::Deposit(collateral) => {
                    println!("depositing {} collateral worth of tokens", collateral);
                    market
                        .exec_mint_and_deposit_liquidity(&lp, collateral.into_number())
                        .unwrap();
                    market.exec_stake_lp(&lp, None).unwrap();

                    if lp_info_before.xlp_amount.into_decimal256() != lp_info_before.xlp_collateral.into_decimal256() {
                        panic!("expected xlp amount to be equal to xlp collateral {:#?}", lp_info_before);
                    }

                    market
                        .exec_farming_deposit_xlp(&lp, NonZero::new(LpToken::from_decimal256(collateral.into_decimal256())).unwrap())
                        .unwrap();
                },
                ActionKind::Withdraw(collateral) => {
                    println!("withdrawing {} collateral worth of tokens", collateral);
                    market
                        .exec_farming_withdraw_xlp(&lp, Some(NonZero::new(FarmingToken::from_decimal256(collateral.into_decimal256())).unwrap()))
                        .unwrap();
                },
            }
        }
        let balance_after_actions = market.query_collateral_balance(&lp).unwrap();

        // farming doesn't affect our collateral balance since we minted as we go
        assert_eq!(balance_after_actions, balance_before_actions);

        Ok(())
    }

    // returns the amount of collateral we got back
    fn unfarm(&self, lp: &Addr) -> Result<Option<Number>> {
        let mut market = self.market.borrow_mut();
        market.automatic_time_jump_enabled = true;
        let balance_before = market.query_collateral_balance(&lp).unwrap();
        let lp_info = market.query_lp_info(&lp).unwrap();

        if lp_info.xlp_amount == LpToken::zero() {
            println!("No xlp to unstake, skipping");
            return Ok(None);
        }

        // begin the xlp unstaking process
        market.exec_unstake_xlp(&lp, None).unwrap();
        // Jump to a long time in the future, so xlp finishes unstaking 
        market.set_time(TimeJump::Hours(365 * 24)).unwrap();

        // convert xlp to lp (i.e. "unstake the xlp")
        market.exec_collect_unstaked_lp(&lp).unwrap();

        // sanity check, shouldn't have any lp or xlp left
        let lp_info_after_unstake = market.query_lp_info(&lp).unwrap();
        assert_eq!(lp_info_after_unstake.xlp_amount, LpToken::zero());
        assert_ne!(lp_info_after_unstake.lp_amount, LpToken::zero());
        assert_eq!(lp_info_after_unstake.unstaking, None);

        // withdraw all liquidity
        market.exec_withdraw_liquidity(&lp, None).unwrap();

        let balance_after_unstake = market.query_collateral_balance(&lp).unwrap();
        let balance_diff_after_unstake = balance_after_unstake - balance_before;

        Ok(Some(balance_diff_after_unstake))
    }
}