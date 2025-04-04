use std::fmt::Display;

use cosmwasm_std::{StdError, StdResult, SubMsg};
use cw_storage_plus::Key;
use cw_storage_plus::{KeyDeserialize, PrimaryKey};
use perpswap::contracts::copy_trading;
use perpswap::contracts::market::{
    order::OrderId,
    position::{PositionId, PositionQueryResponse},
};
use perpswap::{number::Usd, time::Timestamp};

use crate::prelude::*;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) struct MarketInfo {
    /// Market id
    pub(crate) id: MarketId,
    /// Market address
    pub(crate) addr: Addr,
    /// Token used by the market
    pub(crate) token: perpswap::token::Token,
}

pub(crate) struct State<'a> {
    pub(crate) api: &'a dyn Api,
    pub(crate) config: Config,
    pub(crate) querier: QuerierWrapper<'a, Empty>,
    pub(crate) my_addr: Addr,
    pub(crate) env: Env,
}

/// Total LP share information
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
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
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStatus {
    /// Not started Yet
    NotStarted,
    /// The last seen position id. Should be passed to
    /// [perpswap::contracts::position_token::entry::QueryMsg::Tokens]
    ProcessOpenPositions(Option<String>),
    /// Last seen limit order. Should be passed to
    /// [perpswap::contracts::market::entry::QueryMsg::LimitOrders]
    ProcessLimitOrder(Option<OrderId>),
    /// The last seen position id during validate step. Should be passed to
    /// [perpswap::contracts::position_token::entry::QueryMsg::Tokens]
    ValidateOpenPositions {
        /// Start after this position id
        start_after: Option<String>,
        /// Total open positions seen so far
        open_positions: u64,
    },
    /// Last seen limit order during validate step. Should be passed to
    /// [perpswap::contracts::market::entry::QueryMsg::LimitOrders]
    ValidateLimitOrder {
        /// Start after this position id
        start_after: Option<OrderId>,
        /// Total open positions seen so far
        open_orders: u64,
    },
    /// Calculation reset required because a position was opened
    ResetRequired,
    /// Validated that there has been no change in positions
    Validated,
}

impl ProcessingStatus {
    pub fn reset_required(&self) -> bool {
        match self {
            ProcessingStatus::NotStarted => false,
            ProcessingStatus::ProcessOpenPositions(_) => false,
            ProcessingStatus::ProcessLimitOrder(_) => false,
            ProcessingStatus::ResetRequired => true,
            ProcessingStatus::Validated => false,
            ProcessingStatus::ValidateOpenPositions { .. } => false,
            ProcessingStatus::ValidateLimitOrder { .. } => false,
        }
    }

    /// Is this an batch operation status ?
    pub fn is_batch_operation(&self) -> bool {
        match self {
            // This is the initial status
            ProcessingStatus::NotStarted => false,
            // This is set intermediate
            ProcessingStatus::ProcessOpenPositions(_) => true,
            // This is set intermediate
            ProcessingStatus::ProcessLimitOrder(_) => true,
            // This can be a final status
            ProcessingStatus::ResetRequired => false,
            // This can be a final status
            ProcessingStatus::Validated => false,
            ProcessingStatus::ValidateOpenPositions { .. } => true,
            ProcessingStatus::ValidateLimitOrder { .. } => true,
        }
    }

    pub fn not_started_yet(&self) -> bool {
        match self {
            ProcessingStatus::NotStarted => true,
            ProcessingStatus::ProcessOpenPositions(_) => false,
            ProcessingStatus::ProcessLimitOrder(_) => false,
            ProcessingStatus::ResetRequired => false,
            ProcessingStatus::Validated => false,
            ProcessingStatus::ValidateOpenPositions { .. } => false,
            ProcessingStatus::ValidateLimitOrder { .. } => false,
        }
    }

    pub fn is_process_open_positions(&self) -> bool {
        match self {
            ProcessingStatus::NotStarted => false,
            ProcessingStatus::ProcessOpenPositions(_) => true,
            ProcessingStatus::ProcessLimitOrder(_) => false,
            ProcessingStatus::ResetRequired => false,
            ProcessingStatus::Validated => false,
            ProcessingStatus::ValidateOpenPositions { .. } => false,
            ProcessingStatus::ValidateLimitOrder { .. } => false,
        }
    }

    pub fn is_validate_status(&self) -> bool {
        match self {
            ProcessingStatus::NotStarted => false,
            ProcessingStatus::ProcessOpenPositions(_) => false,
            ProcessingStatus::ProcessLimitOrder(_) => false,
            ProcessingStatus::ValidateOpenPositions { .. } => true,
            ProcessingStatus::ValidateLimitOrder { .. } => true,
            ProcessingStatus::ResetRequired => false,
            ProcessingStatus::Validated => false,
        }
    }

    pub fn is_validate_open_position_status(&self) -> bool {
        match self {
            ProcessingStatus::NotStarted => false,
            ProcessingStatus::ProcessOpenPositions(_) => false,
            ProcessingStatus::ProcessLimitOrder(_) => false,
            ProcessingStatus::ValidateOpenPositions { .. } => true,
            ProcessingStatus::ValidateLimitOrder { .. } => false,
            ProcessingStatus::ResetRequired => false,
            ProcessingStatus::Validated => false,
        }
    }
}

/// Specific position information
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub(crate) struct OneLpTokenValue(pub(crate) Collateral);

impl OneLpTokenValue {
    pub(crate) fn collateral_to_shares(
        &self,
        funds: NonZero<Collateral>,
    ) -> Result<NonZero<LpToken>> {
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
#[serde(rename_all = "snake_case")]
pub(crate) struct LpTokenValue {
    /// Value of one LpToken
    pub(crate) value: OneLpTokenValue,
    /// Status of the value
    pub(crate) status: LpTokenStatus,
}

impl LpTokenValue {
    pub(crate) fn set_outdated(&mut self) {
        self.status = LpTokenStatus::Outdated;
    }
}

/// Status of [LpTokenValue]
#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LpTokenStatus {
    /// Recently computed and valid for other computations
    Valid {
        /// Timestamp the value was last computed
        timestamp: Timestamp,
        /// Computed for which queue id
        queue_id: QueuePositionId,
    },
    /// Outdated because of open positions etc. Need to be computed
    /// again.
    #[default]
    Outdated,
}

impl LpTokenStatus {
    pub(crate) fn valid(&self, queue_id: &QueuePositionId) -> bool {
        match self {
            LpTokenStatus::Valid {
                queue_id: self_queue_id,
                ..
            } => self_queue_id == queue_id,
            LpTokenStatus::Outdated => false,
        }
    }
}

/// Queue position pertaining to [crate::state::COLLATERAL_INCREASE_QUEUE]
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) struct IncQueuePosition {
    /// Queue item that needs to be processed
    pub(crate) item: IncQueueItem,
    /// Wallet that initiated the specific item action
    pub(crate) wallet: Addr,
    /// Processing status
    pub(crate) status: copy_trading::ProcessingStatus,
}

/// Queue position pertaining to [crate::state::COLLATERAL_DECREASE_QUEUE]
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DecQueuePosition {
    /// Queue item that needs to be processed
    pub(crate) item: DecQueueItem,
    /// Wallet that initiated the specific item action
    pub(crate) wallet: Addr,
    /// Processing status
    pub(crate) status: copy_trading::ProcessingStatus,
}

impl DecQueuePosition {
    pub fn into_queue_item(self, id: DecQueuePositionId) -> QueueItemStatus {
        QueueItemStatus {
            item: QueueItem::DecCollateral {
                item: Box::new(self.item),
                id,
            },
            status: self.status,
        }
    }
}

impl IncQueuePosition {
    pub fn into_queue_item(self, id: IncQueuePositionId) -> QueueItemStatus {
        QueueItemStatus {
            item: QueueItem::IncCollateral {
                item: self.item,
                id,
            },
            status: self.status,
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
pub(crate) struct OpenPositionsResp {
    /// Fetched tokens
    pub(crate) positions: Vec<PositionQueryResponse>,
}

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
#[serde(rename_all = "snake_case")]
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

/// Leader commission type
#[derive(Debug)]
pub struct LeaderComissision {
    /// Active collateral of the closed position
    pub active_collateral: Collateral,
    /// Total profit made in that closed position
    pub profit: Collateral,
    /// This is the difference between active collateral and commission
    pub remaining_collateral: Collateral,
}

/// High water mark
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HighWaterMark {
    /// Current net profit
    pub current: Signed<Collateral>,
    /// High water mark (HWM)
    pub hwm: Collateral,
}

/// Commission stats for the leader
#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CommissionStats {
    /// Total unclaimed collateral by the leader
    pub unclaimed: Collateral,
    /// Total claimed collateral by the leader. This is cumulative.
    pub claimed: Collateral,
}

/// Comissision that should be paid to the leader
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Commission(pub Collateral);

impl HighWaterMark {
    pub fn add_pnl(&mut self, pnl: Signed<Collateral>, rate: &Decimal256) -> Result<Commission> {
        self.current = self.current.checked_add(pnl)?;
        if self.current <= self.hwm.into_signed() {
            Ok(Commission(Collateral::zero()))
        } else {
            let profit = self.current.checked_sub(self.hwm.into_signed())?;
            self.hwm = self
                .current
                .try_into_non_negative_value()
                .context("Impossible: current is negative")?;
            let commission = profit
                .checked_mul_number(rate.into_signed())?
                .try_into_non_negative_value()
                .context("Impossible: commission is negative")?;
            Ok(Commission(commission))
        }
    }
}

/// Current batch work in Progress
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchWork {
    /// No work present
    NoWork,
    /// Continue Rebalance operation
    BatchRebalance {
        /// Which market id to start from
        start_from: Option<MarketId>,
        /// How much to rebalance
        balance: NonZero<Collateral>,
        /// Token
        token: Token,
    },
    /// Continue LP token computation
    BatchLpTokenValue {
        /// Which market id to start from for the process phase
        process_start_from: Option<MarketId>,
        /// Which market id to start from for the validate phase
        validate_start_from: Option<MarketId>,
        /// Token
        token: Token,
    },
}

/// Helper type for construcing response
pub struct DecQueueResponse {
    /// SubMsg that should be part of response on success case
    pub sub_msg: SubMsg,
    /// Collateral that would be deducted
    pub collateral: NonZero<Collateral>,
    /// Token type for the market actio
    pub token: Token,
    /// Event that should be part of response on successful response
    pub event: Event,
    /// Queue item that is being processed
    pub queue_item: DecQueuePosition,
    /// Corresponding queue id
    pub queue_id: DecQueuePositionId,
    /// Successful reponse
    pub response: Response,
}

/// Same as [DecQueueResponse] but tailed for crank fee
pub struct DecQueueCrankResponse {
    /// SubMsg that should be part of response on success case
    pub sub_msg: SubMsg,
    /// Crank fees
    pub crank_fees: Collateral,
    /// Token type for the market actio
    pub token: Token,
    /// Event that should be part of response on successful response
    pub event: Event,
    /// Queue item that is being processed
    pub queue_item: DecQueuePosition,
    /// Corresponding queue id
    pub queue_id: DecQueuePositionId,
    /// Successful reponse
    pub response: Response,
}

/// Helper type for construcing response
pub struct IncQueueResponse {
    /// SubMsg that should be part of response on success case
    pub sub_msg: SubMsg,
    /// Optional Collateral that could be deducted. Eg: for crank fees.
    pub collateral: Option<Collateral>,
    /// Token type for the market actio
    pub token: Token,
    /// Event that should be part of response on successful response
    pub event: Event,
    /// Queue item that is being processed
    pub queue_item: IncQueuePosition,
    /// Corresponding queue id
    pub queue_id: IncQueuePositionId,
}

/// Process reponse
pub(crate) struct ProcessResponse {
    /// Did it exit early
    pub(crate) early_exit: bool,
    /// Event to emit
    pub(crate) event: Event,
}

/// Crank fee configuration
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CrankFeeConfig {
    /// The crank fee to be paid into the system, in collateral
    pub crank_fee_charged: Usd,
    /// The crank surcharge charged for every 10 items in the deferred execution queue.
    pub crank_fee_surcharge: Usd,
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::Decimal256;
    use perpswap::number::{Collateral, NonZero};

    use crate::types::HighWaterMark;

    use super::OneLpTokenValue;
    use proptest::proptest;

    #[test]
    fn high_water_mark_test() {
        let rate: Decimal256 = "0.1".parse().unwrap();
        let mut hwm = HighWaterMark::default();
        let commission = hwm.add_pnl("100".parse().unwrap(), &rate).unwrap();
        assert_eq!(commission.0, "10".parse().unwrap());
        assert_eq!(hwm.hwm, "100".parse().unwrap());
        assert_eq!(hwm.hwm, hwm.current.try_into_non_negative_value().unwrap());

        hwm.add_pnl("-20".parse().unwrap(), &rate).unwrap();
        assert_eq!(
            hwm.current.try_into_non_negative_value().unwrap(),
            "80".parse().unwrap()
        );
        assert_eq!(hwm.hwm, "100".parse().unwrap());

        let commission = hwm.add_pnl("20".parse().unwrap(), &rate).unwrap();
        assert_eq!(
            hwm.current.try_into_non_negative_value().unwrap(),
            "100".parse().unwrap()
        );
        assert_eq!(hwm.hwm, "100".parse().unwrap());
        assert_eq!(commission.0, Collateral::zero());

        let commission = hwm.add_pnl("20".parse().unwrap(), &rate).unwrap();
        assert_eq!(
            hwm.current.try_into_non_negative_value().unwrap(),
            "120".parse().unwrap()
        );
        assert_eq!(hwm.hwm, "120".parse().unwrap());
        assert_eq!(commission.0, "2".parse().unwrap());

        hwm.add_pnl("-20".parse().unwrap(), &rate).unwrap();
        assert_eq!(
            hwm.current.try_into_non_negative_value().unwrap(),
            "100".parse().unwrap()
        );
        assert_eq!(hwm.hwm, "120".parse().unwrap());

        let commission = hwm.add_pnl("40".parse().unwrap(), &rate).unwrap();
        assert_eq!(
            hwm.current.try_into_non_negative_value().unwrap(),
            "140".parse().unwrap()
        );
        assert_eq!(hwm.hwm, "140".parse().unwrap());
        assert_eq!(commission.0, "2".parse().unwrap());
    }

    #[test]
    fn high_water_mark_negative_test() {
        let rate: Decimal256 = "0.1".parse().unwrap();
        let mut hwm = HighWaterMark::default();

        hwm.add_pnl("-20".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.current, "-20".parse().unwrap());
        assert_eq!(hwm.hwm, "0".parse().unwrap());

        let commission = hwm.add_pnl("20".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.current, "0".parse().unwrap());
        assert_eq!(hwm.hwm, "0".parse().unwrap());
        assert_eq!(commission.0, Collateral::zero());

        let commission = hwm.add_pnl("20".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.current, "20".parse().unwrap());
        assert_eq!(hwm.hwm, "20".parse().unwrap());
        assert_eq!(commission.0, "2".parse().unwrap());
    }

    #[test]
    fn high_water_mark_scenario() {
        let rate: Decimal256 = "0.1".parse().unwrap();
        let mut hwm = HighWaterMark::default();

        let commission = hwm.add_pnl("100".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.current, "100".parse().unwrap());
        assert_eq!(hwm.hwm, "100".parse().unwrap());
        assert_eq!(commission.0, "10".parse().unwrap());

        hwm.add_pnl("-200".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.current, "-100".parse().unwrap());
        assert_eq!(hwm.hwm, "100".parse().unwrap());

        let commission = hwm.add_pnl("80".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.hwm, "100".parse().unwrap());
        assert_eq!(commission.0, "0".parse().unwrap());
        assert_eq!(hwm.current, "-20".parse().unwrap());

        let commission = hwm.add_pnl("20".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.hwm, "100".parse().unwrap());
        assert_eq!(commission.0, "0".parse().unwrap());
        assert_eq!(hwm.current, "0".parse().unwrap());

        hwm.add_pnl("-20".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.hwm, "100".parse().unwrap());
        assert_eq!(hwm.current, "-20".parse().unwrap());

        let commission = hwm.add_pnl("40".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.hwm, "100".parse().unwrap());
        assert_eq!(commission.0, "0".parse().unwrap());
        assert_eq!(hwm.current, "20".parse().unwrap());

        let commission = hwm.add_pnl("120".parse().unwrap(), &rate).unwrap();
        assert_eq!(hwm.hwm, "140".parse().unwrap());
        assert_eq!(commission.0, "4".parse().unwrap());
        assert_eq!(hwm.current, "140".parse().unwrap());
    }

    proptest! {
    #[test]
    fn collateral_to_shares_no_crash(token_value in 0.1f64..2.0, funds in 0.1f64..100.0) {
        fn float_to_collateral(num: f64) -> Collateral {
            let num = num.to_string();
            num.parse().unwrap()
        }
        let token_value = float_to_collateral(token_value);
        let token_value = OneLpTokenValue(token_value);

        let funds = float_to_collateral(funds);
        let funds = NonZero::new(funds).unwrap();
        token_value.collateral_to_shares(funds).unwrap();
      }
    }
}
