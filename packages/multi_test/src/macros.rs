#[macro_export]
macro_rules! return_unless_market_collateral_quote {
    ($market:expr) => {{
        if $market.id.get_market_type() != msg::prelude::MarketType::CollateralIsQuote {
            println!(
                "This test will only pass for a collateral-is-quote market, skipping! {} {}",
                file!(),
                line!()
            );
            return;
        }
    }};
}

#[macro_export]
macro_rules! return_unless_market_collateral_base {
    ($market:expr) => {{
        if $market.id.get_market_type() != msg::prelude::MarketType::CollateralIsBase {
            println!(
                "This test will only pass for a collateral-is-base market, skipping! {} {}",
                file!(),
                line!()
            );
            return;
        }
    }};
}
