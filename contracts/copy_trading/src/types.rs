use std::fmt::Display;

use cosmwasm_std::{StdError, StdResult};
use cw_storage_plus::Key;
use cw_storage_plus::{KeyDeserialize, PrimaryKey};
use msg::contracts::market::{
    deferred_execution::DeferredExecId,
    order::OrderId,
    position::{PositionId, PositionQueryResponse},
};
use shared::{number::Usd, time::Timestamp};

use crate::prelude::*;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct MarketInfo {
    /// Market id
    pub(crate) id: MarketId,
    /// Market address
    pub(crate) addr: Addr,
    /// Token used by the market
    pub(crate) token: msg::token::Token,
}

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) config: Config,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) my_addr: Addr,
}

/// Total LP share information
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
pub(crate) struct Totals {
    /// Total collateral still in this contract.
    ///
    /// Collateral used by active positions is excluded.
    pub(crate) collateral: Collateral,
    /// Total LP shares
    pub(crate) shares: LpToken,
}

/// Market information related to the work performed
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct MarketWorkInfo {
    pub(crate) processing_status: ProcessingStatus,
    /// Total active collateral in all open positions and pending limit orders.
    pub(crate) active_collateral: Collateral,
    /// Total open positions
    pub(crate) count_open_positions: u64,
    /// Total open orders
    pub(crate) count_orders: u64,
}

impl Default for MarketWorkInfo {
    fn default() -> Self {
        Self {
            processing_status: ProcessingStatus::NotStarted,
            active_collateral: Default::default(),
            count_open_positions: Default::default(),
            count_orders: Default::default(),
        }
    }
}

/// Processing Status
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum ProcessingStatus {
    /// Not started Yet
    NotStarted,
    /// The last seen position id. Should be passed to
    /// [msg::contracts::position_token::entry::QueryMsg::Tokens]
    OpenPositions(Option<String>),
    /// The latest deferred exec item we're waiting on. Should be
    /// passed to
    /// [msg::contracts::market::entry::QueryMsg::ListDeferredExecs]
    Deferred(Option<DeferredExecId>),
    /// Last seen limit order. Should be passed to
    /// [msg::contracts::market::entry::QueryMsg::LimitOrders]
    LimitOrder(Option<OrderId>),
    /// Last seen limit order. Should be passed to
    /// [msg::contracts::market::entry::QueryMsg::LimitOrderHistory]
    LimitOrderHistory(Option<String>),
    /// Calculation reset required because a position was opened
    ResetRequired,
    /// Validated that there has been no change in positions
    Validated,
}

impl ProcessingStatus {
    pub fn reset_required(&self) -> bool {
        match self {
            ProcessingStatus::NotStarted => false,
            ProcessingStatus::OpenPositions(_) => false,
            ProcessingStatus::Deferred(_) => false,
            ProcessingStatus::LimitOrder(_) => false,
            ProcessingStatus::LimitOrderHistory(_) => false,
            ProcessingStatus::ResetRequired => true,
            ProcessingStatus::Validated => false,
        }
    }

    pub fn is_validated(&self) -> bool {
        match self {
            ProcessingStatus::NotStarted => false,
            ProcessingStatus::OpenPositions(_) => false,
            ProcessingStatus::Deferred(_) => false,
            ProcessingStatus::LimitOrder(_) => false,
            ProcessingStatus::LimitOrderHistory(_) => false,
            ProcessingStatus::ResetRequired => false,
            ProcessingStatus::Validated => true,
        }
    }
}

/// Specific position information
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct PositionInfo {
    /// Unique identifier for a position
    pub(crate) id: PositionId,
    /// Active collateral for the position
    pub(crate) active_collateral: NonZero<Collateral>,
    /// Unrealized PnL on this position, in terms of collateral.
    pub(crate) pnl_collateral: Signed<Collateral>,
    /// Unrealized PnL on this position, in USD, using cost-basis analysis.
    pub(crate) pnl_usd: Signed<Usd>,
}

/// Specific wallet fund
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct WalletFund {
    /// LP Shares that is locked
    pub(crate) share: NonZero<LpToken>,
    /// Equivalent collateral amount for the LpToken
    pub(crate) collateral: NonZero<Collateral>,
    /// Timestamp locked at
    pub(crate) locked_at: Timestamp,
}

/// Value of one LPToken
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub(crate) struct OneLpTokenValue(pub(crate) Collateral);

impl OneLpTokenValue {
    pub(crate) fn collateral_to_shares(
        &self,
        funds: NonZero<Collateral>,
    ) -> Result<NonZero<LpToken>> {
        // Todo: Write property test for understanding rounding errors
        let new_shares = LpToken::from_decimal256(
            funds
                .raw()
                .checked_div_dec(self.0.into_decimal256())?
                .into_decimal256(),
        );
        NonZero::new(new_shares).context("tokens is zero in collateral_to_shares")
    }

    pub(crate) fn shares_to_collateral(
        &self,
        shares: NonZero<LpToken>,
    ) -> Result<NonZero<Collateral>> {
        let funds = self.0.checked_mul_dec(shares.into_decimal256())?;
        NonZero::new(funds).context("funds is zero in shares_to_collateral")
    }
}

impl Display for OneLpTokenValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// LpToken Value
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub(crate) struct LpTokenValue {
    /// Value of one LpToken
    pub(crate) value: OneLpTokenValue,
    /// Status of the value
    pub(crate) status: LpTokenStatus,
}

/// Status of [LpTokenValue]
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub(crate) enum LpTokenStatus {
    /// Recently computed and valid for other computations
    Valid {
        /// Timestamp the value was last computed
        timestamp: Timestamp,
    },
    /// Outdated because of open positions etc. Need to be computed
    /// again.
    #[default]
    Outdated,
}

impl LpTokenStatus {
    pub(crate) fn valid(&self) -> bool {
        match self {
            LpTokenStatus::Valid { .. } => true,
            LpTokenStatus::Outdated => false,
        }
    }
}

/// Queue position pertaining to [crate::state::COLLATERAL_INCREASE_QUEUE]
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct IncQueuePosition {
    /// Queue item that needs to be processed
    pub(crate) item: IncQueueItem,
    /// Wallet that initiated the specific item action
    pub(crate) wallet: Addr,
}

/// Queue position pertaining to [crate::state::COLLATERAL_DECREASE_QUEUE]
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct DecQueuePosition {
    /// Queue item that needs to be processed
    pub(crate) item: DecQueueItem,
    /// Wallet that initiated the specific item action
    pub(crate) wallet: Addr,
}

impl DecQueuePosition {
    pub fn into_queue_item(self, id: DecQueuePositionId) -> QueueItem {
        QueueItem::DecCollateral {
            item: self.item,
            id,
        }
    }
}

impl IncQueuePosition {
    pub fn into_queue_item(self, id: IncQueuePositionId) -> QueueItem {
        QueueItem::IncCollaleteral {
            item: self.item,
            id,
        }
    }
}

/// Token Response
pub(crate) struct TokenResp {
    /// Fetched tokens
    pub(crate) tokens: Vec<PositionId>,
    /// Start after that should be passed for next iteration
    pub(crate) start_after: Option<String>,
}

/// Open Positions Response
#[allow(dead_code)]
pub(crate) struct OpenPositionsResp {
    /// Fetched tokens
    pub(crate) positions: Vec<PositionQueryResponse>,
    /// Start after that should be passed for next iteration
    pub(crate) start_after: Option<PositionId>,
}

/// Represents total active collateral of open positions in a market
pub(crate) struct PositionCollateral(pub Collateral);

/// Wallet information
#[derive(Clone, Debug)]
pub(crate) struct WalletInfo {
    /// Wallet with this specific token
    pub(crate) token: Token,
    /// Wallet address
    pub(crate) wallet: Addr,
}

impl<'a> PrimaryKey<'a> for WalletInfo {
    type Prefix = Addr;
    type SubPrefix = ();
    type Suffix = Token;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        let mut keys = self.wallet.key();
        keys.extend(self.token.key());
        keys
    }
}

impl KeyDeserialize for WalletInfo {
    type Output = WalletInfo;

    const KEY_ELEMS: u16 = 3;

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        let keys = value.key();
        if keys.len() != 3 {
            return Err(StdError::serialize_err(
                "WalletInfo",
                "WalletInfo keys len is not three",
            ));
        }
        let wallet = keys[0].as_ref();
        let wallet = Addr::from_slice(wallet)?;

        let token_type = &keys[1];
        let token = keys[2].as_ref();
        let token_type = match token_type {
            Key::Val8([token_type]) => token_type,
            _ => return Err(StdError::serialize_err("WalletInfo", "Invalid token type")),
        };
        let token = match token_type {
            0 => {
                let native_token = String::from_slice(token)?;
                Token::Native(native_token)
            }
            1 => {
                let cw20_token = Addr::from_slice(token)?;
                Token::Cw20(cw20_token)
            }
            _ => {
                return Err(StdError::serialize_err(
                    "Token",
                    "Invalid number in token_type",
                ))
            }
        };

        Ok(WalletInfo { token, wallet })
    }
}

/// Status of the market loader
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum MarketLoaderStatus {
    /// Not yet started
    #[default]
    NotStarted,
    /// On going
    OnGoing { last_seen: MarketId },
    /// Finished
    Finished { last_seen: MarketId },
}

impl Display for MarketLoaderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketLoaderStatus::NotStarted => f.write_str("NotStarted"),
            MarketLoaderStatus::OnGoing { last_seen } => write!(f, "Ongoing {}", last_seen),
            MarketLoaderStatus::Finished { last_seen } => write!(f, "Finished {}", last_seen),
        }
    }
}
