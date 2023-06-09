use crate::{
    arbitrary::farming::emissions::data::{Action, ActionKind},
    config::TEST_CONFIG,
    time::TimeJump,
};

use super::data::FarmingEmissions;
use anyhow::Result;
use msg::prelude::*;

impl FarmingEmissions {
    pub fn run(&self) -> Result<()> {
        let lp = self.market.borrow().clone_lp(0).unwrap();

        self.do_simulation(&lp)?;

        if let Some(balance_diff) = self.unfarm(&lp)? {
            println!("balance diff after actions: {}", balance_diff);
            assert_eq!(self.expected_actions_balance_diff(), balance_diff);
        } else if self
            .actions
            .iter()
            .any(|a| matches!(a.kind, ActionKind::Withdraw(_)))
        {
            panic!("expected to unfarm some withdrawals, but didn't have any");
        }

        // if self.actions_farming_balance() > Number::ZERO {
        //     let farmer_info = self.market.borrow().query_farming_farmer_stats(&lp).unwrap();

        //     self.market
        //         .borrow()
        //         .exec_farming_withdraw_xlp(&lp, None)
        //         .unwrap();

        //     let balance_diff = self.unfarm(&lp)?.expect("expected to unfarm some remaining withdrawals at the end, but didn't have any");
        //     println!("balance diff after remainder: {}", balance_diff);
        //     assert_eq!(self.expected_actions_balance_diff(), balance_diff);
        // }

        Ok(())
    }

    // this currently follows-ish the spreadsheet at https://docs.google.com/spreadsheets/d/11JESE0pe_YGUv5ETytlXZw2kR7FZPiKz8KY7Rc2W-zU/edit?usp=sharing
    fn expected_actions_balance_diff(&self) -> Number {
        let emissions_duration = Number::from_ratio_u256(self.emissions_duration_seconds, 1u32);
        let emissions_amount = self.emissions_amount.into_number();
        let emissions_rate = emissions_amount / emissions_duration;

        let data_series_multipliers = {
            let mut data_series_multipliers = Vec::new();
            let mut prev_action: Option<&Action> = None;
            let mut farming_tokens_total = Number::ZERO;

            for action in self.actions.iter() {
                let time_since_last_entry = match prev_action {
                    Some(prev_action) => {
                        Number::from_ratio_u256(action.at_seconds - prev_action.at_seconds, 1u32)
                    }
                    None => Number::ZERO,
                };
                match action.kind {
                    ActionKind::Deposit(collateral) => {
                        farming_tokens_total += collateral.into_number();
                        let data_series_multiplier =
                            ((time_since_last_entry / farming_tokens_total) * emissions_rate)
                                + data_series_multipliers
                                    .last()
                                    .cloned()
                                    .unwrap_or(Number::ZERO);

                        prev_action = Some(action);
                        data_series_multipliers.push(data_series_multiplier);
                    }
                    ActionKind::Withdraw(collateral) => {
                        farming_tokens_total -= collateral.into_number();
                    }
                }
            }

            // push the last data series point for emissions end
            let time_since_last_entry = match prev_action {
                Some(prev_action) => {
                    emissions_duration - Number::from_ratio_u256(prev_action.at_seconds, 1u32)
                }
                None => emissions_duration,
            };

            let data_series_multiplier = ((time_since_last_entry / farming_tokens_total)
                * emissions_rate)
                + data_series_multipliers
                    .last()
                    .cloned()
                    .unwrap_or(Number::ZERO);
            data_series_multipliers.push(data_series_multiplier);

            data_series_multipliers
        };

        let mut farming_balance = Number::ZERO;
        let last_data_series_multiplier = data_series_multipliers
            .last()
            .cloned()
            .unwrap_or(Number::ZERO);

        let mut data_series_index = 0;
        for action in self.actions.iter() {
            match action.kind {
                ActionKind::Deposit(collateral) => {
                    let data_series_multiplier = data_series_multipliers[data_series_index];
                    let data_series_multiplier_diff =
                        last_data_series_multiplier - data_series_multiplier;
                    farming_balance += collateral.into_number() * data_series_multiplier_diff;
                    data_series_index += 1;
                }
                ActionKind::Withdraw(_collateral) => {
                    //farming_balance -= collateral.into_number() * data_series_multiplier_diff;
                }
            }
        }

        println!("data series multipliers: {:?}", data_series_multipliers);

        farming_balance
    }

    fn _actions_farming_balance(&self) -> Number {
        let mut farming_balance = Number::ZERO;
        for action in self.actions.iter() {
            match action.kind {
                ActionKind::Deposit(collateral) => {
                    farming_balance += collateral.into_number();
                }
                ActionKind::Withdraw(collateral) => {
                    farming_balance -= collateral.into_number();
                }
            }
        }

        farming_balance
    }

    fn do_simulation(&self, lp: &Addr) -> Result<()> {
        let mut market = self.market.borrow_mut();
        market.automatic_time_jump_enabled = false;

        market.exec_farming_start_lockdrop(None).unwrap();
        market.set_time(TimeJump::Hours(24 * 365)).unwrap();
        market.exec_farming_start_launch().unwrap();

        let token = market.setup_lvn_rewards(&self.emissions_amount.to_string());

        // // sanity check
        let protocol_owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
        let balance = market.query_reward_token_balance(&token, &protocol_owner);
        assert_eq!(balance, self.emissions_amount);

        market
            .exec_farming_set_emissions(
                market.now(),
                self.emissions_duration_seconds.into(),
                NonZero::new(self.emissions_amount).unwrap(),
                token,
            )
            .unwrap();

        // // Test query farming rewards

        let mut time_jumped = 0;

        let lp_info_before = market.query_lp_info(lp).unwrap();
        let balance_before_actions = market.query_collateral_balance(lp).unwrap();
        for action in self.actions.iter() {
            let seconds = action.at_seconds - time_jumped;
            println!("Jumping {} seconds to {}", seconds, action.at_seconds);
            market.set_time(TimeJump::Seconds(seconds.into())).unwrap();
            time_jumped += seconds;

            match action.kind {
                ActionKind::Deposit(collateral) => {
                    println!("depositing {} collateral worth of tokens", collateral);
                    market
                        .exec_mint_and_deposit_liquidity(lp, collateral.into_number())
                        .unwrap();
                    market.exec_stake_lp(lp, None).unwrap();

                    if lp_info_before.xlp_amount.into_decimal256()
                        != lp_info_before.xlp_collateral.into_decimal256()
                    {
                        panic!(
                            "expected xlp amount to be equal to xlp collateral {:#?}",
                            lp_info_before
                        );
                    }

                    market
                        .exec_farming_deposit_xlp(
                            lp,
                            NonZero::new(LpToken::from_decimal256(collateral.into_decimal256()))
                                .unwrap(),
                        )
                        .unwrap();
                }
                ActionKind::Withdraw(collateral) => {
                    println!("withdrawing {} collateral worth of tokens", collateral);
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

    // returns the amount of collateral we got back
    fn unfarm(&self, lp: &Addr) -> Result<Option<Number>> {
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
