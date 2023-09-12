use msg::prelude::*;
use once_cell::sync::Lazy;
use std::env;
use std::string::ToString;

// Global config
pub struct TestConfig {
    pub native_denom: String,
    pub protocol_owner: String,
    pub migration_admin: String,
    pub dao: String,
    pub kill_switch: String,
    pub wind_down: String,
    pub cw20_decimals: u8,
    pub new_user_funds: Number,
    pub rewards_token_denom: String,
}

pub static TEST_CONFIG: Lazy<TestConfig> = Lazy::new(|| TestConfig {
    native_denom: env::var("NATIVE_DENOM").unwrap_or_else(|_| "native-usd".to_string()),
    protocol_owner: env::var("PROTOCOL_OWNER").unwrap_or_else(|_| "protocol-owner".to_string()),
    migration_admin: env::var("MIGRATION_ADMIN").unwrap_or_else(|_| "migration-admin".to_string()),
    dao: env::var("DAO").unwrap_or_else(|_| "dao".to_string()),
    kill_switch: env::var("KILL_SWITCH").unwrap_or_else(|_| "kill-switch".to_string()),
    wind_down: env::var("WIND_DOWN").unwrap_or_else(|_| "wind-down".to_string()),
    cw20_decimals: env::var("CW20_DECIMALS")
        .unwrap_or_else(|_| "6".to_string())
        .parse()
        .unwrap(),
    new_user_funds: env::var("NEW_USER_FUNDS")
        .unwrap_or_else(|_| "1000000000000".to_string())
        .try_into()
        .unwrap(),
    rewards_token_denom: "REWARDS_DENOM".to_string(),
});

// Config/defaults for the typical scenario of creating a single market at a time
pub struct DefaultMarket {
    pub base: String,
    pub quote: String,
    pub initial_price: PriceBaseInQuote,
    pub cw20_symbol: String,
    pub token_kind: TokenKind,
    pub bootstrap_lp_addr: Addr,
    pub bootstrap_lp_deposit: Number,
    pub collateral_type: MarketType,
}

impl DefaultMarket {
    pub fn market_type() -> MarketType {
        match dotenv::var("MARKET_COLLATERAL_TYPE") {
            Ok(s) => match s.as_str() {
                "quote" => MarketType::CollateralIsQuote,
                "base" => MarketType::CollateralIsBase,
                _ => panic!("env var MARKET_COLLATERAL_TYPE must be either 'quote' or 'base'"),
            },
            Err(_) => MarketType::CollateralIsBase,
        }
    }
}

#[derive(Debug)]
pub enum TokenKind {
    Cw20,
    Native,
}

pub static DEFAULT_MARKET: Lazy<DefaultMarket> = Lazy::new(|| {
    DefaultMarket {
        base: env::var("MARKET_BASE").unwrap_or_else(|_| "ATOM".to_string()),
        quote: env::var("MARKET_QUOTE").unwrap_or_else(|_| "USD".to_string()),
        initial_price: env::var("INITIAL_PRICE")
            .unwrap_or_else(|_| "1".to_string())
            .parse()
            .unwrap(),
        cw20_symbol: env::var("MARKET_CW20_SYMBOL").unwrap_or_else(|_| "contract-usd".to_string()),
        token_kind: {
            let token_kind = match std::env::var("MARKET_TOKEN_KIND") {
                Ok(s) => match s.as_str() {
                    "native" => TokenKind::Native,
                    "cw20" => TokenKind::Cw20,
                    _ => panic!("env var MARKET_TOKEN_KIND must be either 'native' or 'cw20'"),
                },
                Err(_) => TokenKind::Native,
            };

            println!("MARKET_TOKEN_KIND: {:?}", token_kind);

            token_kind
        },
        bootstrap_lp_addr: Addr::unchecked(
            env::var("BOOTSTRAP_LP_ADDR").unwrap_or_else(|_| "bootstrap-lp".to_string()),
        ),
        // tests are tuned to require exactly this amount. don't change it!
        bootstrap_lp_deposit: env::var("BOOTSTRAP_LP_DEPOSIT")
            .unwrap_or_else(|_| "3000".to_string())
            .try_into()
            .unwrap(),
        collateral_type: {
            let market_type = DefaultMarket::market_type();
            println!("MARKET_COLLATERAL_TYPE: {:?}", market_type);
            market_type
        },
    }
});
