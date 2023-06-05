use super::data::{FarmingEmissions, Action, ActionKind};
use msg::prelude::*;
use crate::{
    arbitrary::helpers::token_range_u128, extensions::TokenExt, market_wrapper::PerpsMarket,
    time::TimeJump, PerpsApp,
};
use proptest::prelude::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

impl FarmingEmissions {
    pub fn new_strategy() -> impl Strategy<Value = Self> {
        (10u32..100000u32).prop_flat_map(|emissions_duration_seconds| {
            let mut action_builders:Vec<_> = (0..=10).map(|_| ActionBuilder::new_strategy(emissions_duration_seconds)).collect();
            action_builders.prop_map(move |mut action_builders| {
                let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

                let mut actions:Vec<Action> = Vec::new();
                let mut current_deposit = Collateral::zero();

                action_builders.sort_by(|a, b| a.at_seconds.cmp(&b.at_seconds));

                for action_builder in action_builders {
                    let action = match action_builder.kind {
                        ActionBuilderKind::Deposit(collateral) => {
                            current_deposit += collateral;
                            ActionKind::Deposit(collateral)
                        },
                        ActionBuilderKind::WithdrawPerc(perc) => {
                            let perc:Decimal256 = perc.to_string().parse().unwrap();
                            let collateral = Collateral::from_decimal256(current_deposit.into_decimal256() * perc);
                            current_deposit -= collateral;
                            ActionKind::Withdraw(collateral)
                        },
                    };
                    actions.push(Action {
                        action,
                        at_seconds: action_builder.at_seconds,
                    });
                }


                Self {
                    market: Rc::new(RefCell::new(market)),
                    actions,
                    emissions_duration_seconds,
                }
            })
        })
    }
}

#[derive(Debug, Clone)]
struct ActionBuilder {
    pub at_seconds: u32,
    pub kind: ActionBuilderKind,
}
#[derive(Debug, Clone)]
enum ActionBuilderKind {
    Deposit(Collateral),
    WithdrawPerc(f32),
}

impl ActionBuilder {
    pub fn new_strategy(total_duration_seconds:u32) -> impl Strategy<Value = Self> {
        (0.01f32..0.99f32).prop_flat_map(move |at_perc| {
            let at_seconds = (at_perc * total_duration_seconds as f32) as u32;
            let kind = prop_oneof![
                (1u32..100000u32).prop_map(|collateral| ActionBuilderKind::Deposit(Collateral::from_decimal256(Decimal256::from_ratio(collateral, 1u32)))),
                (0.1f32..0.9f32).prop_map(|perc| ActionBuilderKind::WithdrawPerc(perc)),
            ];
            kind.prop_map(move |kind| Self {
                kind,
                at_seconds,
            })
        })
    }
}