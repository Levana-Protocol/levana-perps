//! Entrypoint messages for the CW20 contract.
use super::Cw20Coin;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128};
use cw_utils::Expiration;
use perpswap::prelude::*;

#[cw_serde]
pub struct InstantiateMsg {
    /************** Cw20 spec *******************/
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_balances: Vec<Cw20Coin>,
    /// We make this mandatory since we always need an owner for these CW20s.
    pub minter: InstantiateMinter,
    pub marketing: Option<InstantiateMarketingInfo>,
}

impl InstantiateMsg {
    pub fn get_cap(&self) -> Option<Uint128> {
        self.minter.cap
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        // Check name, symbol, decimals
        if !self.has_valid_name() {
            perp_bail!(
                ErrorId::MsgValidation,
                ErrorDomain::Cw20,
                "Name is not in the expected format (3-50 UTF-8 bytes)"
            );
        }
        if !self.has_valid_symbol() {
            perp_bail!(
                ErrorId::MsgValidation,
                ErrorDomain::Cw20,
                "Ticker symbol is not in expected format [a-zA-Z\\-]{{3,12}}"
            );
        }
        if self.decimals > 18 {
            perp_bail!(
                ErrorId::MsgValidation,
                ErrorDomain::Cw20,
                "Decimals must not exceed 18"
            );
        }
        if !self.has_valid_balances() {
            perp_bail!(
                ErrorId::MsgValidation,
                ErrorDomain::Cw20,
                "duplicate account balances"
            );
        }
        Ok(())
    }

    fn has_valid_name(&self) -> bool {
        let bytes = self.name.as_bytes();
        if bytes.len() < 3 || bytes.len() > 50 {
            return false;
        }
        true
    }

    fn has_valid_symbol(&self) -> bool {
        let bytes = self.symbol.as_bytes();
        if bytes.len() < 3 || bytes.len() > 12 {
            return false;
        }
        for byte in bytes.iter() {
            if (*byte != 45) && (*byte < 65 || *byte > 90) && (*byte < 97 || *byte > 122) {
                return false;
            }
        }
        true
    }

    fn has_valid_balances(&self) -> bool {
        let mut addresses = self
            .initial_balances
            .iter()
            .map(|c| &c.address)
            .collect::<Vec<_>>();
        addresses.sort();
        addresses.dedup();

        // check for duplicates
        addresses.len() == self.initial_balances.len()
    }
}

#[cw_serde]
pub enum ExecuteMsg {
    /************** Cw20 spec *******************/
    /// Transfer is a base message to move tokens to another account without triggering actions
    Transfer { recipient: RawAddr, amount: Uint128 },
    /// Burn is a base message to destroy tokens forever
    Burn { amount: Uint128 },
    /// Send is a base message to transfer tokens to a contract and trigger an action
    /// on the receiving contract.
    Send {
        contract: RawAddr,
        amount: Uint128,
        msg: Binary,
    },
    /// Allows spender to access an additional amount tokens
    /// from the owner's (env.sender) account. If expires is Some(), overwrites current allowance
    /// expiration with this one.
    IncreaseAllowance {
        spender: RawAddr,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    /// Lowers the spender's access of tokens
    /// from the owner's (env.sender) account by amount. If expires is Some(), overwrites current
    /// allowance expiration with this one.
    DecreaseAllowance {
        spender: RawAddr,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    /// Transfers amount tokens from owner -> recipient
    /// if `env.sender` has sufficient pre-approval.
    TransferFrom {
        owner: RawAddr,
        recipient: RawAddr,
        amount: Uint128,
    },
    /// Sends amount tokens from owner -> contract
    /// if `env.sender` has sufficient pre-approval.
    SendFrom {
        owner: RawAddr,
        contract: RawAddr,
        amount: Uint128,
        msg: Binary,
    },
    /// Destroys tokens forever
    BurnFrom { owner: RawAddr, amount: Uint128 },
    /// If authorized, creates amount new tokens
    /// and adds to the recipient balance.
    Mint { recipient: RawAddr, amount: Uint128 },
    /// This variant is according to spec. The current minter may set
    /// a new minter. Setting the minter to None will remove the
    /// token's minter forever.
    /// there is deliberately *not* a way to set the proprietary MinterKind
    /// so the only way to set the minter to MinterKind::MarketId is at
    /// instantiation
    ///
    /// Note: we require that there always be a minter, so this is not optional!
    UpdateMinter { new_minter: RawAddr },
    /// If authorized, updates marketing metadata.
    /// Setting None/null for any of these will leave it unchanged.
    /// Setting Some("") will clear this field on the contract storage
    UpdateMarketing {
        /// A URL pointing to the project behind this token.
        project: Option<String>,
        /// A longer description of the token and it's utility. Designed for tooltips or such
        description: Option<String>,
        /// The address (if any) who can update this data structure
        marketing: Option<String>,
    },
    /// If set as the "marketing" role on the contract, upload a new URL, SVG, or PNG for the token
    UploadLogo(Logo),
    /************** Proprietary *******************/
    /// Set factory addr
    SetMarket { addr: RawAddr },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /************** Cw20 spec *******************/
    /// * returns [BalanceResponse]
    ///
    /// The current balance of the given address, 0 if unset.
    #[returns(BalanceResponse)]
    Balance { address: RawAddr },

    /// * returns [TokenInfoResponse]
    ///
    /// Returns metadata on the contract - name, decimals, supply, etc.
    #[returns(TokenInfoResponse)]
    TokenInfo {},

    /// * returns [MinterResponse]
    ///
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    #[returns(Option<MinterResponse>)]
    Minter {},

    /// * returns [AllowanceResponse]
    ///
    /// Returns how much spender can use from owner account, 0 if unset.
    #[returns(AllowanceResponse)]
    Allowance { owner: RawAddr, spender: RawAddr },

    /// * returns [AllAllowancesResponse]
    ///
    /// Returns all allowances this owner has approved. Supports pagination.
    #[returns(AllAllowancesResponse)]
    AllAllowances {
        owner: RawAddr,
        start_after: Option<RawAddr>,
        limit: Option<u32>,
    },

    /// * returns [AllSpenderAllowancesResponse]
    ///
    /// Returns all allowances this spender has been granted. Supports pagination.
    #[returns(AllSpenderAllowancesResponse)]
    AllSpenderAllowances {
        spender: RawAddr,
        start_after: Option<RawAddr>,
        limit: Option<u32>,
    },

    /// * returns [AllAccountsResponse]
    ///
    /// Returns all accounts that have balances. Supports pagination.
    #[returns(AllAccountsResponse)]
    AllAccounts {
        start_after: Option<RawAddr>,
        limit: Option<u32>,
    },

    /// * returns [MarketingInfoResponse]
    ///
    /// Returns more metadata on the contract to display in the client:
    /// - description, logo, project url, etc.
    #[returns(MarketingInfoResponse)]
    MarketingInfo {},

    /// * returns [DownloadLogoResponse]
    ///
    /// Downloads the embedded logo data (if stored on chain). Errors if no logo data is stored for this
    /// contract.
    #[returns(DownloadLogoResponse)]
    DownloadLogo {},

    /************** Proprietary *******************/
    /// * returns [cw2::ContractVersion]
    #[returns(cw2::ContractVersion)]
    Version {},
}

/// Placeholder migration message
#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
#[derive(Eq)]
pub struct InstantiateMinter {
    pub minter: RawAddr,
    pub cap: Option<Uint128>,
}

/************** Proprietary but doesn't affect interop *******************/
/************** since queries are according to spec *******************/
/************** and only return addresses *******************/
#[cw_serde]
pub struct InstantiateMarketingInfo {
    pub project: Option<String>,
    pub description: Option<String>,
    pub marketing: Option<Addr>,
    pub logo: Option<Logo>,
}

#[cw_serde]
#[derive(Eq)]
pub struct BalanceResponse {
    pub balance: Uint128,
}

#[cw_serde]
#[derive(Eq)]
pub struct TokenInfoResponse {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}

#[cw_serde]
#[derive(Default)]
pub struct AllowanceResponse {
    pub allowance: Uint128,
    pub expires: Expiration,
}

#[cw_serde]
#[derive(Eq)]
pub struct MinterResponse {
    pub minter: Addr,
    /// cap is a hard cap on total supply that can be achieved by minting.
    /// Note that this refers to total_supply.
    /// If None, there is unlimited cap.
    pub cap: Option<Uint128>,
}

#[cw_serde]
#[derive(Default)]
pub struct MarketingInfoResponse {
    /// A URL pointing to the project behind this token.
    pub project: Option<String>,
    /// A longer description of the token and it's utility. Designed for tooltips or such
    pub description: Option<String>,
    /// A link to the logo, or a comment there is an on-chain logo stored
    pub logo: Option<LogoInfo>,
    /// The address (if any) who can update this data structure
    pub marketing: Option<Addr>,
}

/// When we download an embedded logo, we get this response type.
/// We expect a SPA to be able to accept this info and display it.
#[cw_serde]
pub struct DownloadLogoResponse {
    pub mime_type: String,
    pub data: Binary,
}

#[cw_serde]
pub struct AllowanceInfo {
    pub spender: Addr,
    pub allowance: Uint128,
    pub expires: Expiration,
}

#[cw_serde]
#[derive(Default)]
pub struct AllAllowancesResponse {
    pub allowances: Vec<AllowanceInfo>,
}

#[cw_serde]
pub struct SpenderAllowanceInfo {
    pub owner: Addr,
    pub allowance: Uint128,
    pub expires: Expiration,
}

#[cw_serde]
#[derive(Default)]
pub struct AllSpenderAllowancesResponse {
    pub allowances: Vec<SpenderAllowanceInfo>,
}

#[cw_serde]
#[derive(Default)]
pub struct AllAccountsResponse {
    pub accounts: Vec<Addr>,
}

/// This is used for uploading logo data, or setting it in InstantiateData
#[cw_serde]
pub enum Logo {
    /// A reference to an externally hosted logo. Must be a valid HTTP or HTTPS URL.
    Url(String),
    /// Logo content stored on the blockchain. Enforce maximum size of 5KB on all variants
    Embedded(EmbeddedLogo),
}

/// This is used to store the logo on the blockchain in an accepted format.
/// Enforce maximum size of 5KB on all variants.
#[cw_serde]
pub enum EmbeddedLogo {
    /// Store the Logo as an SVG file. The content must conform to the spec
    /// at <https://en.wikipedia.org/wiki/Scalable_Vector_Graphics>
    ///
    /// (The contract should do some light-weight sanity-check validation)
    Svg(Binary),
    /// Store the Logo as a PNG file. This will likely only support up to 64x64 or so
    /// within the 5KB limit.
    Png(Binary),
}

/// This is used to display logo info, provide a link or inform there is one
/// that can be downloaded from the blockchain itself
#[cw_serde]
pub enum LogoInfo {
    /// A reference to an externally hosted logo. Must be a valid HTTP or HTTPS URL.
    Url(String),
    /// There is an embedded logo on the chain, make another call to download it.
    Embedded,
}

impl From<&Logo> for LogoInfo {
    fn from(logo: &Logo) -> Self {
        match logo {
            Logo::Url(url) => LogoInfo::Url(url.clone()),
            Logo::Embedded(_) => LogoInfo::Embedded,
        }
    }
}
