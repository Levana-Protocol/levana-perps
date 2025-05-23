#![allow(missing_docs)]
pub(crate) const POSITION_OPEN: &str = "position-open";
pub(crate) const POSITION_UPDATE: &str = "position-update";
pub(crate) const POSITION_CLOSE: &str = "position-close";
pub(crate) const POS_ID: &str = "pos-id";
pub(crate) const POS_OWNER: &str = "pos-owner";
pub(crate) const DEPOSIT_COLLATERAL: &str = "deposit-collateral";
pub(crate) const DEPOSIT_COLLATERAL_USD: &str = "deposit-collateral-usd";
pub(crate) const ACTIVE_COLLATERAL: &str = "active-collateral";
pub(crate) const COUNTER_COLLATERAL: &str = "counter-collateral";
pub(crate) const NOTIONAL_SIZE: &str = "notional-size";
pub(crate) const NOTIONAL_SIZE_IN_COLLATERAL: &str = "notional-size-collateral";
pub(crate) const NOTIONAL_SIZE_USD: &str = "notional-size-usd";
pub(crate) const LEVERAGE_TO_BASE: &str = "leverage-to-base";
pub(crate) const MARKET_TYPE: &str = "market-type";
pub(crate) const CREATED_AT: &str = "created-at";
pub(crate) const PRICE_POINT_CREATED_AT: &str = "price-point-created-at";
pub(crate) const LIQUIFUNDED_AT: &str = "liquifunded-at";
pub(crate) const TRADING_FEE: &str = "trading-fee";
pub(crate) const TRADING_FEE_USD: &str = "trading-fee-usd";
pub(crate) const FUNDING_FEE: &str = "funding-fee";
pub(crate) const FUNDING_FEE_USD: &str = "funding-fee-usd";
pub(crate) const BORROW_FEE: &str = "borrow-fee";
pub(crate) const BORROW_FEE_USD: &str = "borrow-fee-usd";
pub(crate) const CRANK_FEE: &str = "crank-fee";
pub(crate) const CRANK_FEE_USD: &str = "crank-fee-usd";
pub(crate) const DELTA_NEUTRALITY_FEE: &str = "delta-neutrality-fee";
pub(crate) const DELTA_NEUTRALITY_FEE_USD: &str = "delta-neutrality-fee-usd";
pub(crate) const UPDATED_AT: &str = "updated-at";
pub(crate) const CLOSED_AT: &str = "closed-at";
pub(crate) const SETTLED_AT: &str = "settled-at";
pub(crate) const CLOSE_REASON: &str = "close-reason";
pub(crate) const STOP_LOSS_OVERRIDE: &str = "stop-loss-override";
// this is generally renamed "take-profit-trader" in the codebase
// but the storage namespace is kept as-is for historical reasons
pub(crate) const TAKE_PROFIT_OVERRIDE: &str = "take-profit-override";
// Being used in multi_test
pub const PLACE_LIMIT_ORDER: &str = "place-limit-order";
pub(crate) const EXECUTE_LIMIT_ORDER: &str = "execute-limit-order";
pub(crate) const EXECUTE_LIMIT_ORDER_ERROR: &str = "error";
// Being used in multi_test
pub const ORDER_ID: &str = "order-id";
pub(crate) const TRIGGER_PRICE: &str = "trigger-price";
pub(crate) const MAX_GAINS: &str = "max-gains";
pub(crate) const PNL: &str = "pnl";
pub(crate) const PNL_USD: &str = "pnl-usd";
pub(crate) const ENTRY_PRICE: &str = "entry-price";

// Delta variants used in PositionUpdateEvent
pub(crate) const DEPOSIT_COLLATERAL_DELTA: &str = "deposit-collateral-delta";
pub(crate) const DEPOSIT_COLLATERAL_DELTA_USD: &str = "deposit-collateral-delta-usd";
pub(crate) const ACTIVE_COLLATERAL_DELTA: &str = "active-collateral-delta";
pub(crate) const ACTIVE_COLLATERAL_DELTA_USD: &str = "active-collateral-delta-usd";
pub(crate) const COUNTER_COLLATERAL_DELTA: &str = "counter-collateral-delta";
pub(crate) const COUNTER_COLLATERAL_DELTA_USD: &str = "counter-collateral-delta-usd";
pub(crate) const NOTIONAL_SIZE_DELTA: &str = "notional-size-delta";
pub(crate) const NOTIONAL_SIZE_DELTA_USD: &str = "notional-size-delta-usd";
pub(crate) const NOTIONAL_SIZE_ABS_DELTA: &str = "notional-size-abs-delta";
pub(crate) const NOTIONAL_SIZE_ABS_DELTA_USD: &str = "notional-size-abs-delta-usd";
pub(crate) const LEVERAGE_DELTA: &str = "leverage-delta";
pub(crate) const COUNTER_LEVERAGE_DELTA: &str = "counter-leverage-delta";
pub(crate) const TRADING_FEE_DELTA: &str = "trading-fee-delta";
pub(crate) const TRADING_FEE_DELTA_USD: &str = "trading-fee-delta-usd";
pub(crate) const DELTA_NEUTRALITY_FEE_DELTA: &str = "delta-neutrality-fee-delta";
pub(crate) const DELTA_NEUTRALITY_FEE_DELTA_USD: &str = "delta-neutrality-fee-delta-usd";

pub(crate) const DIRECTION: &str = "direction";
pub(crate) const LEVERAGE: &str = "leverage";
pub(crate) const COUNTER_LEVERAGE: &str = "counter-leverage";

// history stuff
pub(crate) const POSITION_ACTION_KIND: &str = "kind";
pub(crate) const POSITION_ACTION_TIMESTAMP: &str = "timestamp";
pub(crate) const POSITION_ACTION_PRICE_TIMESTAMP: &str = "price-timestamp";
pub(crate) const POSITION_ACTION_COLLATERAL: &str = "collateral";
pub(crate) const POSITION_ACTION_TRANSFER: &str = "transfer";
pub(crate) const POSITION_ACTION_LEVERAGE: &str = "leverage";
pub(crate) const POSITION_ACTION_MAX_GAINS: &str = "max-gains";
pub(crate) const POSITION_ACTION_TRADE_FEE: &str = "trade-fee";
pub(crate) const POSITION_ACTION_DELTA_NEUTRALITY_FEE: &str = "delta-neutrality-fee";
pub(crate) const POSITION_ACTION_OLD_OWNER: &str = "old-owner";
pub(crate) const POSITION_ACTION_NEW_OWNER: &str = "new-owner";

pub(crate) const LP_ACTION_KIND: &str = "kind";
pub(crate) const LP_ACTION_ID: &str = "action-id";
pub(crate) const LP_ACTION_ADDRESS: &str = "addr";
pub(crate) const LP_ACTION_TIMESTAMP: &str = "timestamp";
pub(crate) const LP_ACTION_TOKENS: &str = "tokens";
pub(crate) const LP_ACTION_COLLATERAL: &str = "collateral";
pub(crate) const LP_ACTION_COLLATERAL_USD: &str = "collateral-usd";

pub(crate) const INSUFFICIENT_MARGIN: &str = "insufficient-margin";
pub(crate) const FEE_TYPE: &str = "fee-type";
pub(crate) const AVAILABLE: &str = "available";
pub(crate) const REQUESTED: &str = "requested";
pub(crate) const DESC: &str = "desc";

// pub(crate) const GRANT_REWARDS: &str = "grant-rewards";
// pub(crate) const CLAIM_REWARDS: &str = "claim-rewards";
// pub(crate) const REWARDS_RECIPIENT: &str = "rewards-recipient";
// pub(crate) const REWARDS_AMOUNT: &str = "rewards-amount";
pub(crate) const DEFERRED_EXEC_ID: &str = "deferred-exec-id";
pub(crate) const DEFERRED_EXEC_OWNER: &str = "deferred-exec-owner";
pub(crate) const DEFERRED_EXEC_TARGET: &str = "deferred-exec-target";
pub(crate) const SUCCESS: &str = "success";

pub(crate) const LIQUIDATION_MARGIN_BORROW: &str = "liquidation-margin-borrow";
pub(crate) const LIQUIDATION_MARGIN_FUNDING: &str = "liquidation-margin-funding";
pub(crate) const LIQUIDATION_MARGIN_DNF: &str = "liquidation-margin-dnf";
pub(crate) const LIQUIDATION_MARGIN_CRANK: &str = "liquidation-margin-crank";
pub(crate) const LIQUIDATION_MARGIN_EXPOSURE: &str = "liquidation-margin-exposure";
pub(crate) const LIQUIDATION_MARGIN_TOTAL: &str = "liquidation-margin-total";
