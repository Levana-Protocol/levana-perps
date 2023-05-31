use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use shared::prelude::*;

use crate::contracts::cw20::Cw20Coin;

#[cw_serde]
pub struct InstantiateMsg {
    /// Given in seconds
    pub tap_limit: Option<u32>,
    /// Code ID of the CW20 contract we'll deploy
    pub cw20_code_id: u64,
    /// Configuration of the gas coin allowance
    pub gas_allowance: Option<GasAllowance>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Tap {
        assets: Vec<FaucetAsset>,
        recipient: RawAddr,
        amount: Option<Number>,
    },
    Multitap {
        recipients: Vec<MultitapRecipient>,
    },
    OwnerMsg(OwnerMsg),
}

#[cw_serde]
pub struct MultitapRecipient {
    pub addr: RawAddr,
    pub assets: Vec<FaucetAsset>,
}

#[cw_serde]
pub enum FaucetAsset {
    Cw20(RawAddr),
    Native(String),
}

#[cw_serde]
pub enum OwnerMsg {
    AddAdmin {
        admin: RawAddr,
    },
    RemoveAdmin {
        admin: RawAddr,
    },
    /// Given in seconds
    SetTapLimit {
        tap_limit: Option<u32>,
    },
    SetTapAmount {
        asset: FaucetAsset,
        amount: Number,
    },
    DeployToken {
        /// Name of the asset, used as both CW20 name and symbol. Example: `ATOM`.
        name: String,
        tap_amount: Number,
        /// Each trading competition token for an asset is assigned an index to
        /// disambiguate them. It also makes it easier to find the token you
        /// just created with a deploy. These are intended to be monotonically
        /// increasing. When deploying a new trading competition token, consider
        /// using [QueryMsg::NextTradingIndex] to find the next available
        /// number.
        ///
        /// By providing [None], you're saying that you want to deploy an
        /// unrestricted token which can be tapped multiple times and be used
        /// with any contract.
        trading_competition_index: Option<u32>,
        initial_balances: Vec<Cw20Coin>,
    },
    SetMarketAddress {
        name: String,
        trading_competition_index: u32,
        market: RawAddr,
    },
    SetCw20CodeId {
        cw20_code_id: u64,
    },
    Mint {
        cw20: String,
        balances: Vec<Cw20Coin>,
    },
    SetGasAllowance {
        allowance: GasAllowance,
    },
    ClearGasAllowance {},
    /// Set the tap amount for a named asset
    SetMultitapAmount {
        name: String,
        amount: Decimal256,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [cw2::ContractVersion]
    #[returns(cw2::ContractVersion)]
    Version {},

    /// * returns [ConfigResponse]
    #[returns(ConfigResponse)]
    Config {},

    /// * returns [GetTokenResponse]
    #[returns(GetTokenResponse)]
    GetToken {
        name: String,
        trading_competition_index: Option<u32>,
    },

    /// Returns the next trading competition index we can use for the given asset name
    ///
    /// * returns [NextTradingIndexResponse]
    #[returns(NextTradingIndexResponse)]
    NextTradingIndex { name: String },

    /// * returns [GasAllowanceResp]
    #[returns(GasAllowanceResp)]
    GetGasAllowance {},

    /// * returns [TapEligibleResponse]
    #[returns(TapEligibleResponse)]
    IsTapEligible {
        addr: RawAddr,
        #[serde(default)]
        assets: Vec<FaucetAsset>,
    },

    /// * returns [IsAdminResponse]
    #[returns(IsAdminResponse)]
    IsAdmin { addr: RawAddr },

    /// * returns [TapAmountResponse]
    #[returns(TapAmountResponse)]
    TapAmount { asset: FaucetAsset },

    /// * returns [TapAmountResponse]
    #[returns(TapAmountResponse)]
    TapAmountByName { name: String },

    /// Find out the cumulative amount of funds transferred at a given timestamp.
    #[returns(FundsSentResponse)]
    FundsSent {
        asset: FaucetAsset,
        timestamp: Option<Timestamp>,
    },

    /// Enumerate all wallets that tapped the faucet
    #[returns(TappersResp)]
    Tappers {
        start_after: Option<RawAddr>,
        limit: Option<u32>,
    },
}

#[cw_serde]
pub enum GetTokenResponse {
    Found { address: Addr },
    NotFound {},
}

#[cw_serde]
pub struct NextTradingIndexResponse {
    pub next_index: u32,
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct ConfigResponse {
    pub admins: Vec<Addr>,
    /// Given in seconds
    pub tap_limit: Option<u32>,
}

#[cw_serde]
pub struct GasAllowance {
    pub denom: String,
    pub amount: Uint128,
}

#[cw_serde]
pub enum GasAllowanceResp {
    Enabled { denom: String, amount: Uint128 },
    Disabled {},
}

#[cw_serde]
pub enum TapEligibleResponse {
    Eligible {},
    Ineligible {
        seconds: Decimal256,
        message: String,
        reason: IneligibleReason,
    },
}

#[cw_serde]
pub enum IneligibleReason {
    TooSoon,
    AlreadyTapped,
}

#[cw_serde]
pub struct IsAdminResponse {
    pub is_admin: bool,
}

#[cw_serde]
pub enum TapAmountResponse {
    CannotTap {},
    CanTap { amount: Decimal256 },
}

#[cw_serde]
pub struct FundsSentResponse {
    pub amount: Decimal256,
}

#[cw_serde]
pub struct TappersResp {
    pub tappers: Vec<Addr>,
}
