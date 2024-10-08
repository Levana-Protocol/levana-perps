use anyhow::Context;
use levana_perpswap_multi_test::{
    market_wrapper::PerpsMarket, return_unless_market_collateral_base, PerpsApp,
};
use perpswap::bridge::ClientToBridgeWrapper;

#[test]
fn diagnostic_log_take_profit_less_than_collateral() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    return_unless_market_collateral_base!(market);

    run_log(
        market,
        include_str!("../diagnostic-logs/take_profit_less_than_collateral_small.txt"),
    );
}

fn run_log(market: PerpsMarket, body: &str) {
    body.lines()
        .map(|line| line.trim())
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(linenum, line)| {
            serde_json::from_str::<ClientToBridgeWrapper>(line)
                .with_context(|| format!("Parsing line #{linenum}"))
                .unwrap()
        })
        .for_each(|wrapper| {
            market.handle_bridge_msg(
                &wrapper,
                |_exec_resp| {
                    // if let Err(e) = exec_resp {
                    //     panic!("Execute Error: {:#?} Wrapper Message was: {:#?}", e, wrapper);
                    // }
                },
                |_query_resp| {
                    // if let Err(e) = query_resp {
                    //     panic!("Query Error: {:#?} Wrapper Message was: {:#?}", e, wrapper);
                    // }
                },
                |_time_jump_resp| {
                    // if let Err(e) = time_jump_resp {
                    //     panic!("Timejump Error: {:#?} Wrapper Message was: {:#?}", e, wrapper);
                    // }
                },
            );
        });
}
