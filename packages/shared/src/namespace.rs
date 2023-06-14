//! This is a centralized location for cw_storage Item storage keys and Map namespaces
#![allow(missing_docs)]

pub const FACTORY_ADDR: &str = "a";
pub const MARKET_PRICE_ADMINS: &str = "b";
pub const MIGRATION_ADMIN: &str = "c";
pub const OWNER_ADDR: &str = "d";
pub const CONFIG: &str = "e";
pub const LAST_CRANK_COMPLETED: &str = "f";
pub const MINTER: &str = "g";
pub const MINTER_CAP: &str = "h";
pub const TOKEN_INFO: &str = "i";
pub const MARKETING_INFO: &str = "j";
pub const LOGO: &str = "k";
pub const TAP_LIMIT: &str = "l";
pub const ALL_FEES: &str = "m";
// pub const PROTOCOL_FEES: &str = "n";
pub const TOKEN_KIND: &str = "o";
// pub const TOTAL_LP_SHARES: &str = "p";
pub const LIQUIDITY_STATS: &str = "q";
pub const LIQUIDITY_TOKEN_CODE_ID: &str = "r";
pub const MARKET_CODE_ID: &str = "s";
pub const MARKET_ID: &str = "t";
pub const LAST_POSITION_ID: &str = "u";
// pub const TOTAL_NOTIONAL_OPEN: &str = "v";
pub const OPEN_NOTIONAL_LONG_INTEREST: &str = "w";
pub const OPEN_NOTIONAL_SHORT_INTEREST: &str = "x";
// pub const TOTAL_NOTIONAL_PENDING_OPEN: &str = "y";
// pub const TOTAL_NOTIONAL_PENDING_CLOSE: &str = "z";
pub const POSITION_TOKEN_CODE_ID: &str = "aa";
pub const REPLY_INSTANTIATE_MARKET: &str = "ab";
pub const TOKEN: &str = "ac";
pub const NFT_COUNT: &str = "ad";
pub const BALANCES: &str = "ae";
pub const ALLOWANCES: &str = "af";
pub const ALLOWANCES_SPENDER: &str = "ag";
pub const LAST_TAP_TIMESTAMP: &str = "ah";
pub const CW20_TOKEN_INFO: &str = "ai";
pub const CW20_TAP_AMOUNT: &str = "aj";
pub const NATIVE_TAP_AMOUNT: &str = "ak";
pub const LIQUIDITY_STATS_BY_ADDR: &str = "al";
pub const YIELD_PER_TIME_PER_TOKEN: &str = "am";
// pub const LP_YIELD_INFO: &str = "an";
pub const LP_ADDRS: &str = "ao";
pub const XLP_ADDRS: &str = "ap";
pub const LP_ADDRS_REVERSE: &str = "aq";
pub const XLP_ADDRS_REVERSE: &str = "ar";
pub const MARKET_ADDRS: &str = "as";
pub const PRICES: &str = "at";
pub const OPEN_POSITIONS: &str = "au";
pub const CLOSED_POSITIONS: &str = "av";
// pub const LIQUIDATION_PRICES: &str = "aw";
pub const PRICE_TRIGGER_DESC: &str = "ax";
pub const PRICE_TRIGGER_ASC: &str = "ay";
// pub const TAKE_PROFIT_PRICES_LONG: &str = "az";
// pub const TAKE_PROFIT_PRICES_SHORT: &str = "ba";
pub const LIQUIDATION_PRICES_PENDING: &str = "bb";
pub const LIQUIDATION_PRICES_PENDING_REVERSE: &str = "bc";
pub const CLOSED_POSITION_HISTORY: &str = "bd";
pub const NEXT_LIQUIFUNDING: &str = "be";
pub const NEXT_STALE: &str = "bf";
pub const POSITION_TOKEN_ADDRS: &str = "bg";
pub const NFT_APPROVALS: &str = "bh";
pub const NFT_OPERATORS: &str = "bi";
pub const NFT_OWNERS: &str = "bj";
pub const NFT_POSITION_IDS: &str = "bk";
pub const LP_BORROW_FEE_DATA_SERIES: &str = "bl";
pub const BORROW_FEE_DATA_SERIES_LEN: &str = "bm";
pub const FUNDING_RATE_LONG: &str = "bn";
pub const FUNDING_RATE_LONG_LEN: &str = "bo";
pub const FUNDING_RATE_SHORT: &str = "bp";
pub const FUNDING_RATE_SHORT_LEN: &str = "bq";
// pub const TC_IS_ENABLED: &str = "br";
pub const TC_MARKET_ADDRESS: &str = "bs";
pub const TC_TAPPED_ONCE: &str = "bt";
pub const ALL_CONTRACTS: &str = "bu";
// pub const DEPOSITED_XLP: &str = "bv";
// pub const UNSTAKING_XLP: &str = "bw";
pub const FAUCET_TOKEN_INFO: &str = "bx";
pub const CW20_CODE_ID: &str = "by";
pub const FAUCET_TOKENS: &str = "bz";
pub const FAUCET_TOKENS_TRADE: &str = "ca";
pub const FAUCET_NEXT_TOKEN: &str = "cb";
pub const DAO_ADDR: &str = "cc";
// pub const SANITY_UNALLOCATED_FUNDS: &str = "cd";
// pub const SANITY_COLLATERAL_FUNDS: &str = "ce";
// pub const SANITY_FEE_FUNDS: &str = "cf";
// pub const SANITY_LIQUIDITY_FUNDS: &str = "cg";
pub const XLP_BORROW_FEE_DATA_SERIES: &str = "ch";
pub const WITHDRAW_INFO: &str = "ci";
pub const TIME_UNFROZEN: &str = "cj";
pub const WITHDRAW_TOTALS: &str = "ck";
pub const WITHDRAW_END: &str = "cl";
pub const NEXT_FREEZE: &str = "cm";
pub const TRADE_HISTORY_SUMMARY: &str = "cn";
pub const TRADE_HISTORY_BY_POSITION: &str = "co";
pub const TRADE_HISTORY_BY_ADDRESS: &str = "cp";
pub const LP_HISTORY_SUMMARY: &str = "cq";
pub const LP_HISTORY_BY_ADDRESS: &str = "cr";
pub const LP_HISTORY_STATUS_XLP_LP: &str = "cs";
pub const LP_HISTORY_STATUS_LP_COLLATERAL: &str = "ct";
pub const RESET_LP_STATUS: &str = "cu";
pub const WIND_DOWN_ADDR: &str = "cv";
pub const KILL_SWITCH_ADDR: &str = "cw";
pub const SHUTDOWNS: &str = "cx";
pub const CLOSE_ALL_POSITIONS: &str = "cy";
pub const LAST_ORDER_ID: &str = "cz";
pub const LIMIT_ORDERS: &str = "da";
pub const LIMIT_ORDERS_REVERSE: &str = "db";
pub const LIMIT_ORDERS_BY_PRICE_LONG: &str = "dc";
pub const LIMIT_ORDERS_BY_PRICE_SHORT: &str = "de";
pub const LIMIT_ORDERS_BY_ADDR: &str = "df";
pub const DELTA_NEUTRALITY_FUND: &str = "dg";
// pub const SANITY_DELTA_NEUTRALITY_FUNDS: &str = "dh";
// pub const PENDING_CRANK_FEES: &str = "di";
pub const TOTAL_NET_FUNDING_PAID: &str = "dj";
pub const TOTAL_FUNDING_MARGIN: &str = "dk";
pub const LP_ALLOWANCES: &str = "dl";
pub const LP_ALLOWANCES_SPENDER: &str = "dm";
pub const XLP_ALLOWANCES: &str = "dn";
pub const XLP_ALLOWANCES_SPENDER: &str = "do";
pub const LABEL_SUFFIX: &str = "dp";
pub const EXECUTED_LIMIT_ORDERS: &str = "dq";
pub const PYTH_ADDR: &str = "dr";
pub const PYTH_MARKET_PRICE_FEEDS: &str = "ds";
pub const PYTH_UPDATE_AGE_TOLERANCE: &str = "dt";
pub const PYTH_PREV_MARKET_PRICE: &str = "du";
pub const LIQUIDATION_PRICES_PENDING_COUNT: &str = "dv";
pub const LVN_TOKEN: &str = "dw";
pub const LVN_LOCKDROP_REWARDS: &str = "dx";
pub const REWARDS_PER_TIME_PER_TOKEN: &str = "dy";
pub const LVN_EMISSIONS: &str = "dz";
pub const LOCKDROP_CONFIG: &str = "ea";
pub const LOCKDROP_BUCKETS_MULTIPLIER: &str = "eb";
pub const LOCKDROP_BUCKETS_DURATION: &str = "ec";
pub const LOCKDROP_BUCKETS_BALANCES: &str = "ed";
pub const LOCKDROP_BUCKETS_TOTAL_SHARES: &str = "ee";
pub const LOCKDROP_DURATIONS: &str = "ef";
pub const BONUS_CONFIG: &str = "eg";
pub const EPHEMERAL_BONUS_FUND: &str = "eh";
pub const BONUS_FUND: &str = "ei";
pub const EPHEMERAL_DEPOSIT_COLLATERAL_DATA: &str = "ej";
pub const RECLAIMABLE_EMISSIONS: &str = "ek";
pub const LOCKDROP_BALANCES_BY_BUCKET: &str = "el";
