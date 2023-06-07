//! Entrypoint messages for the market
use super::order::LimitOrder;
use super::position::{ClosedPosition, PositionId};
use super::{config::ConfigUpdate, crank::CrankWorkInfo};
use crate::contracts::market::order::OrderId;
use crate::{contracts::liquidity_token::LiquidityTokenKind, token::TokenInit};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Decimal256, Uint128};
use shared::prelude::*;

/// The InstantiateMsg comes from Factory only
#[cw_serde]
pub struct InstantiateMsg {
    /// The factory address
    pub factory: RawAddr,
    /// Modifications to the default config value
    pub config: Option<ConfigUpdate>,
    /// Base, quote, and market type
    pub market_id: MarketId,
    /// The token used for collateral
    pub token: TokenInit,
    /// Initial borrow fee rate when launching the protocol, annualized
    pub initial_borrow_fee_rate: Decimal256,
}

/// Config info passed on to all sub-contracts in order to
/// add a new market.
#[cw_serde]
pub struct NewMarketParams {
    /// Base, quote, and market type
    pub market_id: MarketId,

    /// The token used for collateral
    pub token: TokenInit,

    /// config
    pub config: Option<ConfigUpdate>,

    /// The address of the price admin for this market
    pub price_admin: RawAddr,

    /// Initial borrow fee rate, annualized
    pub initial_borrow_fee_rate: Decimal256,
}

/// There are two sources of slippage in the protocol:
/// - Change in the oracle price from creation of the message to execution of the message.
/// - Change in delta neutrality fee from creation of the message to execution of the message.
/// Slippage assert tolerance is the tolerance to the sum of the two sources of slippage.
#[cw_serde]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SlippageAssert {
    /// Expected effective price from the sender. To incorporate tolerance on delta neutrality fee,
    /// the expected price should be modified by expected fee rate:
    /// `price = oracle_price * (1 + fee_rate)`
    /// `fee_rate` here is the ratio between the delta neutrality fee amount and notional size delta (in collateral asset).
    pub price: PriceBaseInQuote,
    /// Max ratio tolerance of actual trade price differing in an unfavorable direction from expected price.
    /// Tolerance of 0.01 means max 1% difference.
    pub tolerance: Number,
}

/// Execute message for the market contract
#[allow(clippy::large_enum_variant)]
#[cw_serde]
pub enum ExecuteMsg {
    /// Owner-only executions
    Owner(ExecuteOwnerMsg),

    /// cw20
    Receive {
        /// Owner of funds sent to the contract
        sender: RawAddr,
        /// Amount of funds sent
        amount: Uint128,
        /// Must parse to a [ExecuteMsg]
        msg: Binary,
    },

    /// Open a new position
    OpenPosition {
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
        /// Leverage of new position
        leverage: LeverageToBase,
        /// Direction of new position
        direction: DirectionToBase,
        /// Maximum gains of new position
        max_gains: MaxGainsInQuote,
        /// Stop loss price of new position
        stop_loss_override: Option<PriceBaseInQuote>,
        /// Take profit price of new position
        take_profit_override: Option<PriceBaseInQuote>,
    },

    /// Add collateral to a position, causing leverage to decrease
    ///
    /// The amount of collateral to add must be attached as funds
    UpdatePositionAddCollateralImpactLeverage {
        /// ID of position to update
        id: PositionId,
    },

    /// Add collateral to a position, causing notional size to increase
    ///
    /// The amount of collateral to add must be attached as funds
    UpdatePositionAddCollateralImpactSize {
        /// ID of position to update
        id: PositionId,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
    },

    /// Remove collateral from a position, causing leverage to increase
    UpdatePositionRemoveCollateralImpactLeverage {
        /// ID of position to update
        id: PositionId,
        /// Amount of funds to remove from the position
        amount: NonZero<Collateral>,
    },
    /// Remove collateral from a position, causing notional size to decrease
    UpdatePositionRemoveCollateralImpactSize {
        /// ID of position to update
        id: PositionId,
        /// Amount of funds to remove from the position
        amount: NonZero<Collateral>,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
    },

    /// Modify the leverage of the position
    ///
    /// This will impact the notional size of the position
    UpdatePositionLeverage {
        /// ID of position to update
        id: PositionId,
        /// New leverage of the position
        leverage: LeverageToBase,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
    },

    /// Modify the max gains of a position
    UpdatePositionMaxGains {
        /// ID of position to update
        id: PositionId,
        /// New max gains of the position
        max_gains: MaxGainsInQuote,
    },

    /// Set a stop loss or take profit override.
    /// This msg will override any previous values.
    /// Passing None will remove the override.
    SetTriggerOrder {
        /// ID of position to modify
        id: PositionId,
        /// New stop loss price of the position
        stop_loss_override: Option<PriceBaseInQuote>,
        /// New take profit price of the position
        take_profit_override: Option<PriceBaseInQuote>,
    },

    /// Set a limit order to open a position when the price of the asset hits
    /// the specified trigger price.
    PlaceLimitOrder {
        /// Price when the order should trigger
        trigger_price: PriceBaseInQuote,
        /// Leverage of new position
        leverage: LeverageToBase,
        /// Direction of new position
        direction: DirectionToBase,
        /// Max gains of new position
        max_gains: MaxGainsInQuote,
        /// Stop loss price of new position
        stop_loss_override: Option<PriceBaseInQuote>,
        /// Take profit price of new position
        take_profit_override: Option<PriceBaseInQuote>,
    },

    /// Cancel an open limit order
    CancelLimitOrder {
        /// ID of the order
        order_id: OrderId,
    },

    /// Close a position
    ClosePosition {
        /// ID of position to close
        id: PositionId,
        /// Assertion that the price has not moved too far
        slippage_assert: Option<SlippageAssert>,
    },

    /// Deposits send funds into the unlocked liquidity fund
    /// Returns [LiquidityDepositResponseData] as response data
    DepositLiquidity {
        /// Should we stake the resulting LP tokens into xLP?
        ///
        /// Defaults to `false`.
        #[serde(default)]
        stake_to_xlp: bool,
    },

    /// Like [ExecuteMsg::DepositLiquidity], but reinvests pending yield instead of receiving new funds.
    ReinvestYield {
        /// Should we stake the resulting LP tokens into xLP?
        ///
        /// Defaults to `false`.
        #[serde(default)]
        stake_to_xlp: bool,
        /// Amount of rewards to reinvest.
        ///
        /// If `None`, reinvests all pending rewards.
        amount: Option<NonZero<Collateral>>,
    },

    /// Withdraw liquidity calculated from specified `lp_amount`
    WithdrawLiquidity {
        /// Amount of LP tokens to burn
        lp_amount: Option<NonZero<LpToken>>,
    },

    /// Claims accrued yield based on LP share allocation
    ClaimYield {},

    /// Stake some existing LP tokens into xLP
    ///
    /// [None] means stake all LP tokens.
    StakeLp {
        /// Amount of LP tokens to convert into xLP.
        amount: Option<NonZero<LpToken>>,
    },

    /// Begin unstaking xLP into LP
    ///
    /// [None] means unstake all xLP tokens.
    UnstakeXlp {
        /// Amount of xLP tokens to convert into LP
        amount: Option<NonZero<LpToken>>,
    },

    /// Stop an ongoing xLP unstaking process.
    StopUnstakingXlp {},

    /// Collect any LP tokens that have been unstaked from xLP.
    CollectUnstakedLp {},

    /// Crank a number of times
    Crank {
        /// Total number of crank executions to do
        /// None: config default
        execs: Option<u32>,
        /// Which wallet receives crank rewards.
        ///
        /// If unspecified, sender receives the rewards.
        rewards: Option<RawAddr>,
    },

    /// Nft proxy messages.
    /// Only allowed to be called by this market's position_token contract
    NftProxy {
        /// Original caller of the NFT proxy.
        sender: RawAddr,
        /// Message sent to the NFT proxy
        msg: crate::contracts::position_token::entry::ExecuteMsg,
    },

    /// liquidity token cw20 proxy messages.
    /// Only allowed to be called by this market's liquidity_token contract
    LiquidityTokenProxy {
        /// Original caller of the liquidity token proxy.
        sender: RawAddr,
        /// Whether this was the LP or xLP proxy.
        kind: LiquidityTokenKind,
        /// Message sent to the liquidity token proxy.
        msg: crate::contracts::liquidity_token::entry::ExecuteMsg,
    },

    /// Updates the price of base asset in terms of quote.
    /// This msg is permissioned.
    SetPrice {
        /// Price of the base asset in terms of the quote asset
        price: PriceBaseInQuote,
        /// Price of the collateral asset in terms of USD
        ///
        /// This is used by the protocol to track USD values. This field is
        /// optional, as markets with USD as the quote asset do not need to
        /// provide it.
        price_usd: Option<PriceCollateralInUsd>,
        /// How many executions of the crank to perform
        ///
        /// Each time a price is updated in the system, cranking is immediately
        /// necessary to check for liquidations. As an optimization, the
        /// protocol includes that cranking as part of price updating. The value
        /// here represents how many turns of the crank should be performed, or
        /// use [None] for the default.
        execs: Option<u32>,
        /// Which wallet receives crank rewards.
        ///
        /// If unspecified, sender receives the rewards.
        rewards: Option<RawAddr>,
    },

    /// Transfer all available protocol fees to the dao account
    TransferDaoFees {},

    /// Begin force-closing all positions in the protocol.
    ///
    /// This can only be performed by the market wind down wallet.
    CloseAllPositions {},

    /// Provide funds directly to the crank fees.
    ///
    /// The person who calls this receives no benefits. It's intended for the
    /// DAO to use to incentivize cranking.
    ProvideCrankFunds {},
}

/// Owner-only messages
#[cw_serde]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum ExecuteOwnerMsg {
    /// Update the config
    ConfigUpdate {
        /// New configuration parameters
        update: ConfigUpdate,
    },
}

/// Fees held within the market contract.
#[cw_serde]
pub struct Fees {
    /// Fees available for individual wallets to withdraw.
    pub wallets: Collateral,
    /// Fees available for the protocol overall to withdraw.
    pub protocol: Collateral,
    /// Crank fees collected and waiting to be allocated to crankers.
    pub crank: Collateral,
}

/// Return value from [QueryMsg::ClosedPositionHistory]
#[cw_serde]
pub struct ClosedPositionsResp {
    /// Closed positions
    pub positions: Vec<ClosedPosition>,
    /// the next cursor to start from
    /// if we've reached the end, it's a None
    pub cursor: Option<ClosedPositionCursor>,
}

/// A cursor used for paginating
/// the closed position history
#[cw_serde]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ClosedPositionCursor {
    /// Last close timestamp
    pub time: Timestamp,
    /// Last closed position ID
    pub position: PositionId,
}

/// Query messages on the market contract
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [cw2::ContractVersion]
    #[returns(cw2::ContractVersion)]
    Version {},

    /// Provides overall information about this market.
    ///
    /// This is intended as catch-all for protocol wide information, both static
    /// (like market ID) and dynamic (like notional interest). The goal is to
    /// limit the total number of queries callers have to make to get relevant
    /// information.
    ///
    /// * returns [StatusResp]
    #[returns(StatusResp)]
    Status {},

    /// * returns [shared::prelude::PricePoint]
    ///
    /// Gets the spot price, if no time is supplied, then it's current
    #[returns(shared::prelude::PricePoint)]
    SpotPrice {
        /// Timestamp when the price should be effective.
        ///
        /// [None] means "give the most recent price."
        timestamp: Option<Timestamp>,
    },

    /// * returns [SpotPriceHistoryResp]
    ///
    /// Gets a collection of historical spot prices
    #[returns(SpotPriceHistoryResp)]
    SpotPriceHistory {
        /// Last timestamp we saw
        start_after: Option<Timestamp>,
        /// How many prices to query
        limit: Option<u32>,
        /// Order to sort by, if None then it will be descending
        order: Option<OrderInMessage>,
    },

    /// * returns [super::position::PositionsResp]
    ///
    /// Maps the given PositionIds into Positions
    #[returns(super::position::PositionsResp)]
    Positions {
        /// Positions to query.
        position_ids: Vec<PositionId>,
        /// Should we skip calculating pending fees?
        #[serde(default)]
        skip_calc_pending_fees: bool,
    },

    /// * returns [LimitOrderResp]
    ///
    /// Returns the specified Limit Order
    #[returns(LimitOrderResp)]
    LimitOrder {
        /// Limit order ID to query
        order_id: OrderId,
    },

    /// * returns [LimitOrdersResp]
    ///
    /// Returns the Limit Orders for the specified addr
    #[returns(LimitOrdersResp)]
    LimitOrders {
        /// Owner of limit orders
        owner: RawAddr,
        /// Last limit order seen
        start_after: Option<OrderId>,
        /// Number of order to return
        limit: Option<u32>,
        /// Whether to return ascending or descending
        order: Option<OrderInMessage>,
    },

    /// * returns [ClosedPositionsResp]
    #[returns(ClosedPositionsResp)]
    ClosedPositionHistory {
        /// Owner of the positions to get history for
        owner: RawAddr,
        /// Cursor to start from, for pagination
        cursor: Option<ClosedPositionCursor>,
        /// limit pagination
        limit: Option<u32>,
        /// order is default Descending
        order: Option<OrderInMessage>,
    },

    /// * returns [cosmwasm_std::QueryResponse]
    ///
    /// Nft proxy messages. Not meant to be called directly
    /// but rather for internal cross-contract calls
    ///
    /// however, these are merely queries, and can be called by anyone
    /// and clients may take advantage of this to save query gas
    /// by calling the market directly
    #[returns(cosmwasm_std::QueryResponse)]
    NftProxy {
        /// NFT message to process
        nft_msg: crate::contracts::position_token::entry::QueryMsg,
    },

    /// * returns [cosmwasm_std::QueryResponse]
    ///
    /// Liquidity token cw20 proxy messages. Not meant to be called directly
    /// but rather for internal cross-contract calls
    ///
    /// however, these are merely queries, and can be called by anyone
    /// and clients may take advantage of this to save query gas
    /// by calling the market directly
    #[returns(cosmwasm_std::QueryResponse)]
    LiquidityTokenProxy {
        /// Whether to query LP or xLP tokens
        kind: LiquidityTokenKind,
        /// Query to run
        msg: crate::contracts::liquidity_token::entry::QueryMsg,
    },

    /// * returns [TradeHistorySummary] for a given wallet addr
    #[returns(TradeHistorySummary)]
    TradeHistorySummary {
        /// Which wallet's history are we querying?
        addr: RawAddr,
    },

    /// * returns [PositionActionHistoryResp]
    #[returns(PositionActionHistoryResp)]
    PositionActionHistory {
        /// Which position's history are we querying?
        id: PositionId,
        /// Last action ID we saw
        start_after: Option<String>,
        /// How many actions to query
        limit: Option<u32>,
        /// Order to sort by
        order: Option<OrderInMessage>,
    },

    /// Actions taken by a trader.
    ///
    /// Similar to [Self::PositionActionHistory], but provides all details for
    /// an individual trader, not an individual position.
    ///
    /// * returns [TraderActionHistoryResp]
    #[returns(TraderActionHistoryResp)]
    TraderActionHistory {
        /// Which trader's history are we querying?
        owner: RawAddr,
        /// Last action ID we saw
        start_after: Option<String>,
        /// How many actions to query
        limit: Option<u32>,
        /// Order to sort by
        order: Option<OrderInMessage>,
    },

    /// * returns [LpActionHistoryResp]
    #[returns(LpActionHistoryResp)]
    LpActionHistory {
        /// Which provider's history are we querying?
        addr: RawAddr,
        /// Last action ID we saw
        start_after: Option<String>,
        /// How many actions to query
        limit: Option<u32>,
        /// Order to sort by
        order: Option<OrderInMessage>,
    },

    /// * returns [LimitOrderHistoryResp]
    ///
    /// Provides information on triggered limit orders.
    #[returns(LimitOrderHistoryResp)]
    LimitOrderHistory {
        /// Trader's address for history we are querying
        addr: RawAddr,
        /// Last order ID we saw
        start_after: Option<String>,
        /// How many orders to query
        limit: Option<u32>,
        /// Order to sort the order IDs by
        order: Option<OrderInMessage>,
    },

    /// * returns [LpInfoResp]
    ///
    /// Provides the data needed by the earn page.
    #[returns(LpInfoResp)]
    LpInfo {
        /// Which provider's information are we querying?
        liquidity_provider: RawAddr,
    },

    /// * returns [DeltaNeutralityFeeResp]
    ///
    /// Gets the delta neutrality fee
    /// at the current price, for a given change in terms of net notional
    #[returns(DeltaNeutralityFeeResp)]
    DeltaNeutralityFee {
        /// the amount of notional that would be changed
        notional_delta: Signed<Notional>,
        /// for real delta neutrality fees, this is calculated internally
        /// should only be supplied if querying the fee for close or update
        pos_delta_neutrality_fee_margin: Option<Collateral>,
    },
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}

/// The summary for trade history
#[cw_serde]
#[derive(Default)]
pub struct TradeHistorySummary {
    /// Given in usd
    pub trade_volume: Usd,
    /// Given in usd
    pub realized_pnl: Signed<Usd>,
}

/// Response for [QueryMsg::PositionActionHistory]
#[cw_serde]
pub struct PositionActionHistoryResp {
    /// list of position actions that happened historically
    pub actions: Vec<PositionAction>,
    /// Next start_after value to continue pagination
    ///
    /// None means no more pagination
    pub next_start_after: Option<String>,
}

/// Response for [QueryMsg::TraderActionHistory]
#[cw_serde]
pub struct TraderActionHistoryResp {
    /// list of position actions that this trader performed
    pub actions: Vec<PositionAction>,
    /// Next start_after value to continue pagination
    ///
    /// None means no more pagination
    pub next_start_after: Option<String>,
}

/// A distinct position history action
#[cw_serde]
pub struct PositionAction {
    /// ID of the position impacted
    ///
    /// For ease of migration, we allow for a missing position ID.
    pub id: Option<PositionId>,
    /// Kind of action taken by the trader
    pub kind: PositionActionKind,
    /// Timestamp when the action occurred
    pub timestamp: Timestamp,
    /// the amount of collateral at the time of the action
    pub collateral: Collateral,
    /// Leverage of the position at the time of the action, if relevant
    pub leverage: Option<LeverageToBase>,
    /// max gains in quote
    pub max_gains: Option<MaxGainsInQuote>,
    /// the trade fee in USD
    pub trade_fee: Option<Usd>,
    /// The delta neutrality fee paid (or, if negative, received) in USD
    pub delta_neutrality_fee: Option<Signed<Usd>>,
    /// If this is a position transfer, the previous owner.
    pub old_owner: Option<Addr>,
    /// If this is a position transfer, the new owner.
    pub new_owner: Option<Addr>,
}

/// Action taken by trader for a [PositionAction]
#[cw_serde]
pub enum PositionActionKind {
    /// Open a new position
    Open,
    /// Updated an existing position
    Update,
    /// Close a position
    Close,
    /// Position was transferred between wallets
    Transfer,
}

/// Returned by [QueryMsg::LpInfo]
#[cw_serde]
pub struct LpInfoResp {
    /// This LP amount includes both actual LP tokens and xLP unstaked to LP but
    /// not yet collected.
    pub lp_amount: LpToken,
    /// Collateral backing the LP tokens
    pub lp_collateral: Collateral,
    /// This shows the balance of xLP minus any xLP already unstaked.
    pub xlp_amount: LpToken,
    /// Collateral backing the xLP tokens
    pub xlp_collateral: Collateral,
    /// Total available yield, sum of the available LP, xLP, and crank rewards.
    pub available_yield: Collateral,
    /// Available yield from LP tokens
    pub available_yield_lp: Collateral,
    /// Available yield from xLP tokens
    pub available_yield_xlp: Collateral,
    /// Available crank rewards
    pub available_crank_rewards: Collateral,
    /// Current status of an unstaking, if under way
    ///
    /// This will return `Some` from the time the provider begins an unstaking process until either:
    ///
    /// 1. They either cancel it, _or_
    /// 2. They unstake all request xLP into LP _and_ collect that LP within the contract.
    pub unstaking: Option<UnstakingStatus>,
    /// Historical information on LP activity
    pub history: LpHistorySummary,
}

/// Status of an ongoing unstaking process.
#[cw_serde]
pub struct UnstakingStatus {
    /// When the unstaking began
    pub start: Timestamp,
    /// This will be in the future if unstaking is incomplete
    pub end: Timestamp,
    /// Total amount requested to be unstaked
    ///
    /// Note that this value must be the sum of collected, available, and pending.
    pub xlp_unstaking: NonZero<LpToken>,
    /// Collateral, at current exchange rate, underlying the [UnstakingStatus::xlp_unstaking]
    pub xlp_unstaking_collateral: Collateral,
    /// Total amount of LP tokens that have been unstaked and collected
    pub collected: LpToken,
    /// Total amount of LP tokens that have been unstaked and not yet collected
    pub available: LpToken,
    /// Total amount of xLP tokens that are still pending unstaking
    pub pending: LpToken,
}

/// The summary for LP history
#[cw_serde]
#[derive(Default)]
pub struct LpHistorySummary {
    /// How much collateral was deposited in total
    pub deposit: Collateral,
    /// Value of the collateral in USD at time of deposit
    #[serde(alias = "deposit_in_usd")]
    pub deposit_usd: Usd,
    /// Cumulative yield claimed by the provider
    pub r#yield: Collateral,
    /// Cumulative yield expressed in USD at time of claiming
    #[serde(alias = "yield_in_usd")]
    pub yield_usd: Usd,
}

/// Response for [QueryMsg::LpActionHistory]
#[cw_serde]
pub struct LpActionHistoryResp {
    /// list of earn actions that happened historically
    pub actions: Vec<LpAction>,
    /// Next start_after value to continue pagination
    ///
    /// None means no more pagination
    pub next_start_after: Option<String>,
}

/// A distinct lp history action
#[cw_serde]
pub struct LpAction {
    /// Kind of action
    pub kind: LpActionKind,
    /// When the action happened
    pub timestamp: Timestamp,
    /// How many tokens were involved, if relevant
    pub tokens: Option<LpToken>,
    /// Amount of collateral
    pub collateral: Collateral,
    /// Value of that collateral in USD at the time
    #[serde(alias = "collateral_in_usd")]
    pub collateral_usd: Usd,
}

/// Kind of action for a [LpAction].
#[cw_serde]
pub enum LpActionKind {
    /// via [ExecuteMsg::DepositLiquidity]
    DepositLp,
    /// via [ExecuteMsg::DepositLiquidity]
    DepositXlp,
    /// via [ExecuteMsg::ReinvestYield]
    ReinvestYieldLp,
    /// via [ExecuteMsg::ReinvestYield]
    ReinvestYieldXlp,
    /// via [ExecuteMsg::UnstakeXlp]
    /// the amount of collateral is determined by the time they send their message
    /// [ExecuteMsg::CollectUnstakedLp] is *not* accounted for here
    UnstakeXlp,
    /// Some amount of unstaked LP has been collected into actual LP.
    CollectLp,
    /// via [ExecuteMsg::WithdrawLiquidity]
    Withdraw,
    /// via [ExecuteMsg::ClaimYield]
    ClaimYield,
}

#[cw_serde]
/// Return value from [QueryMsg::LimitOrder].
pub struct LimitOrderResp {
    /// The order identifier
    pub order_id: OrderId,
    /// The price at which the order will trigger
    pub trigger_price: PriceBaseInQuote,
    /// Amount of deposit collateral on the order
    pub collateral: NonZero<Collateral>,
    /// Leverage to open the position at
    pub leverage: LeverageToBase,
    /// Direction of the new position
    pub direction: DirectionToBase,
    /// Max gains of the new position
    pub max_gains: MaxGainsInQuote,
    /// Stop loss of the new position
    pub stop_loss_override: Option<PriceBaseInQuote>,
    /// Take profit of the new position
    pub take_profit_override: Option<PriceBaseInQuote>,
}

/// Response for [QueryMsg::LimitOrders]
#[cw_serde]
pub struct LimitOrdersResp {
    /// The list of limit orders
    pub orders: Vec<LimitOrderResp>,
    /// Next start_after value to continue pagination
    ///
    /// None means no more pagination
    pub next_start_after: Option<OrderId>,
}

/// Response for [QueryMsg::DeltaNeutralityFee]
#[cw_serde]
pub struct DeltaNeutralityFeeResp {
    /// the amount charged
    pub amount: Signed<Collateral>,
    /// the amount in the fund currently
    pub fund_total: Collateral,
    /// Expected effective price after slippage, can be used for the slippage assert.
    pub slippage_assert_price: PriceBaseInQuote,
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for QueryMsg {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        Self::arbitrary_with_user(u, None)
    }
}

#[cfg(feature = "arbitrary")]
impl QueryMsg {
    /// Generate an arbitrary [QueryMsg] using the given default user address.
    pub fn arbitrary_with_user(
        u: &mut arbitrary::Unstructured<'_>,
        user: Option<RawAddr>,
    ) -> arbitrary::Result<Self> {
        let user_arb = |u: &mut arbitrary::Unstructured<'_>| -> arbitrary::Result<RawAddr> {
            match user {
                Some(user) => Ok(user),
                None => u.arbitrary(),
            }
        };
        // only allow messages for *this* contract - no proxies or cw20 submessages

        // prior art for this approach: https://github.com/rust-fuzz/arbitrary/blob/061ca86be699faf1fb584dd7a7843b3541cd5f2c/src/lib.rs#L724
        match u.int_in_range::<u8>(0..=11)? {
            0 => Ok(Self::Version {}),
            1 => Ok(Self::Status {}),
            2 => Ok(Self::SpotPrice {
                timestamp: u.arbitrary()?,
            }),
            3 => Ok(Self::Positions {
                position_ids: u.arbitrary()?,
                skip_calc_pending_fees: u.arbitrary()?,
            }),

            4 => Ok(Self::LimitOrder {
                order_id: u.arbitrary()?,
            }),

            5 => Ok(Self::LimitOrders {
                owner: user_arb(u)?,
                start_after: u.arbitrary()?,
                limit: u.arbitrary()?,
                order: u.arbitrary()?,
            }),

            6 => Ok(Self::ClosedPositionHistory {
                owner: user_arb(u)?,
                cursor: u.arbitrary()?,
                limit: u.arbitrary()?,
                order: u.arbitrary()?,
            }),

            7 => Ok(Self::TradeHistorySummary { addr: user_arb(u)? }),

            8 => Ok(Self::PositionActionHistory {
                id: u.arbitrary()?,
                start_after: u.arbitrary()?,
                limit: u.arbitrary()?,
                order: u.arbitrary()?,
            }),

            9 => Ok(Self::LpActionHistory {
                addr: user_arb(u)?,
                start_after: u.arbitrary()?,
                limit: u.arbitrary()?,
                order: u.arbitrary()?,
            }),

            10 => Ok(Self::LpInfo {
                liquidity_provider: user_arb(u)?,
            }),

            11 => Ok(Self::DeltaNeutralityFee {
                notional_delta: u.arbitrary()?,
                pos_delta_neutrality_fee_margin: u.arbitrary()?,
            }),

            _ => unreachable!(),
        }
    }
}

#[cfg(feature = "arbitrary")]
impl<'a> arbitrary::Arbitrary<'a> for ExecuteMsg {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        // only allow messages for *this* contract - no proxies or cw20 submessages

        // prior art for this approach: https://github.com/rust-fuzz/arbitrary/blob/061ca86be699faf1fb584dd7a7843b3541cd5f2c/src/lib.rs#L724
        match u.int_in_range::<u8>(0..=24)? {
            //0 => Ok(ExecuteMsg::Owner(u.arbitrary()?)),
            0 => Ok(ExecuteMsg::Owner(ExecuteOwnerMsg::ConfigUpdate {
                update: ConfigUpdate::default(),
            })),
            1 => Ok(ExecuteMsg::OpenPosition {
                slippage_assert: u.arbitrary()?,
                leverage: u.arbitrary()?,
                direction: u.arbitrary()?,
                max_gains: u.arbitrary()?,
                stop_loss_override: u.arbitrary()?,
                take_profit_override: u.arbitrary()?,
            }),
            2 => Ok(ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { id: u.arbitrary()? }),
            3 => Ok(ExecuteMsg::UpdatePositionAddCollateralImpactSize {
                id: u.arbitrary()?,
                slippage_assert: u.arbitrary()?,
            }),
            4 => Ok(ExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage {
                id: u.arbitrary()?,
                amount: u.arbitrary()?,
            }),
            5 => Ok(ExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                id: u.arbitrary()?,
                amount: u.arbitrary()?,
                slippage_assert: u.arbitrary()?,
            }),
            6 => Ok(ExecuteMsg::UpdatePositionLeverage {
                id: u.arbitrary()?,
                leverage: u.arbitrary()?,
                slippage_assert: u.arbitrary()?,
            }),
            7 => Ok(ExecuteMsg::UpdatePositionMaxGains {
                id: u.arbitrary()?,
                max_gains: u.arbitrary()?,
            }),
            8 => Ok(ExecuteMsg::SetTriggerOrder {
                id: u.arbitrary()?,
                stop_loss_override: u.arbitrary()?,
                take_profit_override: u.arbitrary()?,
            }),
            9 => Ok(ExecuteMsg::PlaceLimitOrder {
                trigger_price: u.arbitrary()?,
                leverage: u.arbitrary()?,
                direction: u.arbitrary()?,
                max_gains: u.arbitrary()?,
                stop_loss_override: u.arbitrary()?,
                take_profit_override: u.arbitrary()?,
            }),
            10 => Ok(ExecuteMsg::CancelLimitOrder {
                order_id: u.arbitrary()?,
            }),
            11 => Ok(ExecuteMsg::ClosePosition {
                id: u.arbitrary()?,
                slippage_assert: u.arbitrary()?,
            }),
            12 => Ok(ExecuteMsg::DepositLiquidity {
                stake_to_xlp: u.arbitrary()?,
            }),
            13 => Ok(ExecuteMsg::ReinvestYield {
                stake_to_xlp: u.arbitrary()?,
                amount: None,
            }),
            14 => Ok(ExecuteMsg::WithdrawLiquidity {
                lp_amount: u.arbitrary()?,
            }),
            15 => Ok(ExecuteMsg::ClaimYield {}),
            16 => Ok(ExecuteMsg::StakeLp {
                amount: u.arbitrary()?,
            }),
            17 => Ok(ExecuteMsg::UnstakeXlp {
                amount: u.arbitrary()?,
            }),
            18 => Ok(ExecuteMsg::StopUnstakingXlp {}),
            19 => Ok(ExecuteMsg::CollectUnstakedLp {}),
            20 => Ok(ExecuteMsg::Crank {
                execs: u.arbitrary()?,
                rewards: None,
            }),

            21 => Ok(ExecuteMsg::SetPrice {
                price: u.arbitrary()?,
                price_usd: u.arbitrary()?,
                execs: u.arbitrary()?,
                rewards: None,
            }),

            22 => Ok(ExecuteMsg::TransferDaoFees {}),

            23 => Ok(ExecuteMsg::CloseAllPositions {}),
            24 => Ok(ExecuteMsg::ProvideCrankFunds {}),

            _ => unreachable!(),
        }
    }
}

/// Overall market status information
///
/// Returned from [QueryMsg::Status]
#[cw_serde]
pub struct StatusResp {
    /// This market's identifier
    pub market_id: MarketId,
    /// Base asset
    pub base: String,
    /// Quote asset
    pub quote: String,
    /// Type of market
    pub market_type: MarketType,
    /// The asset used for collateral within the system
    pub collateral: crate::token::Token,
    /// Config for this market
    pub config: super::config::Config,
    /// Current status of the liquidity pool
    pub liquidity: super::liquidity::LiquidityStats,
    /// Next bit of crank work available, if any
    pub next_crank: Option<CrankWorkInfo>,
    /// Timestamp of the last completed crank
    pub last_crank_completed: Option<Timestamp>,
    /// Size of the unpend queue
    pub unpend_queue_size: u32,
    /// Overall borrow fee rate (annualized), combining LP and xLP
    pub borrow_fee: Decimal256,
    /// LP component of [Self::borrow_fee]
    pub borrow_fee_lp: Decimal256,
    /// xLP component of [Self::borrow_fee]
    pub borrow_fee_xlp: Decimal256,
    /// Long funding rate (annualized)
    pub long_funding: Number,
    /// Short funding rate (annualized)
    pub short_funding: Number,

    /// Total long interest, given in the notional asset.
    pub long_notional: Notional,
    /// Total short interest, given in the notional asset.
    pub short_notional: Notional,

    /// Total long interest, given in USD, converted at the current exchange rate.
    pub long_usd: Usd,
    /// Total short interest, given in USD, converted at the current exchange rate.
    pub short_usd: Usd,

    /// Instant delta neutrality fee value
    ///
    /// This is based on net notional and the sensitivity parameter
    pub instant_delta_neutrality_fee_value: Signed<Decimal256>,

    /// Amount of collateral in the delta neutrality fee fund.
    pub delta_neutrality_fee_fund: Collateral,

    /// Have we reached staleness of the protocol via old liquifundings? If so, contains [Option::Some], and the timestamp when that happened.
    pub stale_liquifunding: Option<Timestamp>,
    /// Is the last price update too old? If so, contains [Option::Some], and the timestamp when the price became too old.
    pub stale_price: Option<Timestamp>,
    /// Are we in the congested state where new positions cannot be opened?
    pub congested: bool,

    /// Fees held by the market contract
    pub fees: Fees,
}

impl StatusResp {
    /// Is the protocol stale from either liquifunding delay or old prices?
    pub fn is_stale(&self) -> bool {
        self.stale_liquifunding.is_some() || self.stale_price.is_some()
    }
}

/// Response for [QueryMsg::LimitOrderHistory]
#[cw_serde]
pub struct LimitOrderHistoryResp {
    /// list of triggered limit orders that happened historically
    pub orders: Vec<ExecutedLimitOrder>,
    /// Next start_after value to continue pagination
    ///
    /// None means no more pagination
    pub next_start_after: Option<String>,
}

/// History information on a limit order which was triggered.
#[cw_serde]
pub struct ExecutedLimitOrder {
    /// The order itself
    pub order: LimitOrder,
    /// The result of triggering the order
    pub result: LimitOrderResult,
    /// When the order was triggered
    pub timestamp: Timestamp,
}

/// The result of triggering a limit order
#[cw_serde]
pub enum LimitOrderResult {
    /// Position was opened successfully
    Success {
        /// New position ID
        position: PositionId,
    },
    /// Position failed to open
    Failure {
        /// Error message
        reason: String,
    },
}

/// Response for [QueryMsg::SpotPriceHistory]
#[cw_serde]
pub struct SpotPriceHistoryResp {
    /// list of historical price points
    pub price_points: Vec<PricePoint>,
}
