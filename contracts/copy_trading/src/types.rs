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

/// LpToken Value
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub(crate) struct LpTokenValue {
    /// Value of one LpToken
    pub(crate) value: Collateral,
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

/// Queue position
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct QueuePosition {
    /// Queue item that needs to be processed
    pub(crate) item: QueueItem,
    /// Wallet that initiated the specific item action
    pub(crate) wallet: Addr,
}

impl QueuePosition {
    pub fn into_queue_resp_item(self, id: QueuePositionId) -> QueueRespItem {
        QueueRespItem {
            id,
            item: self.item,
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
    type Prefix = Token;
    type SubPrefix = ();
    type Suffix = Addr;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        let mut keys = self.token.key();
        keys.extend(self.wallet.key());
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
        let token_type = &keys[0];
        let token = keys[1].as_ref();
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
        let wallet = keys[3].as_ref();
        let wallet = Addr::from_slice(wallet)?;
        Ok(WalletInfo { token, wallet })
    }
}
