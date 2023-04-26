#![no_main]

use anyhow::Result;
use arbitrary::Arbitrary;
use cosmwasm_std::Addr;
use libfuzzer_sys::fuzz_target;
use msg::{
    contracts::market::entry::{ExecuteMsg, QueryMsg},
    prelude::*,
};
use multi_test::{config::TEST_CONFIG, market_wrapper::PerpsMarket, PerpsApp};

thread_local! {
    static MARKET: PerpsMarket = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
}

#[derive(Debug)]
enum MarketRound {
    Exec {
        msg: Box<ExecuteMsg>,
        collateral: Option<NumberGtZero>,
    },
    Query(Box<QueryMsg>),
}

impl<'a> Arbitrary<'a> for MarketRound {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        if u.arbitrary::<bool>()? {
            let msg: ExecuteMsg = u.arbitrary()?;
            let has_collateral: bool = match msg {
                // variants that require collateral
                ExecuteMsg::OpenPosition { .. } => true,
                ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { .. } => true,
                ExecuteMsg::UpdatePositionAddCollateralImpactSize { .. } => true,
                ExecuteMsg::DepositLiquidity { .. } => true,
                ExecuteMsg::ReinvestYield { .. } => true,
                ExecuteMsg::ProvideCrankFunds { .. } => true,
                _ => false,
            };

            let collateral: Option<NumberGtZero> = if has_collateral {
                Some(u.arbitrary()?)
            } else {
                None
            };

            Ok(Self::Exec {
                msg: Box::new(msg),
                collateral,
            })
        } else {
            Ok(Self::Query(Box::new(QueryMsg::arbitrary_with_user(
                u,
                Some(TEST_CONFIG.protocol_owner.clone().into()),
            )?)))
        }
    }
}

fuzz_target!(|round: MarketRound| {
    MARKET.with(|market| match round {
        MarketRound::Exec { msg, collateral } => {
            let res = match collateral {
                None => market.exec(&Addr::unchecked(&TEST_CONFIG.protocol_owner), &msg),
                Some(collateral) => market.exec_funds(
                    &Addr::unchecked(&TEST_CONFIG.protocol_owner),
                    &msg,
                    collateral.into_number(),
                ),
            };

            handle_result("exec", res);
        }
        MarketRound::Query(query_msg) => {
            handle_result("query", market.raw_query(&query_msg));
        }
    });
});

fn handle_result<T>(_context: &str, result: Result<T>) {
    if let Err(err) = result {
        if err.downcast_ref::<PerpError>().is_none() {
            // for now we allow *all* native contract errors
            // in the future we could investigate this further
            // to see if it's really an expected error
            // or if we have some checked math or whatever that's failing unexpectedly
            // panic!("ERROR IN {}: {:?}", _context, err);
        }
    }
}
