//! Entrypoint messages for the farming contract.
pub mod defaults;

use crate::prelude::*;
use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{Binary, Uint128};

/// Instantiate a new farming contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Owner wallet allowed to perform [ExecuteMsg::Owner] actions.
    pub owner: RawAddr,
    /// Factory contract we work with
    pub factory: RawAddr,
    /// Market ID within the factory
    pub market_id: MarketId,
    /// How many seconds a a lockdrop "month" lasts.
    #[serde(default = "defaults::lockdrop_month_seconds")]
    pub lockdrop_month_seconds: u32,
    /// Lockdrop buckets supported by this contracts
    #[serde(default = "defaults::lockdrop_buckets")]
    pub lockdrop_buckets: Vec<LockdropBucketConfig>,
    /// The amount of real yield taken for the bonus fund
    #[serde(default = "defaults::bonus_ratio")]
    pub bonus_ratio: NonZero<Decimal256>,
    /// The address that receives bonus transfers.
    pub bonus_addr: RawAddr,
    /// How long LVN rewards from the lockdrop take to linearly unlock
    #[serde(default = "defaults::lockdrop_month_seconds")]
    pub lockdrop_lvn_unlock_seconds: u32,
    /// What ratio of lockdrop LVN becomes available immediately on launch
    #[serde(default = "defaults::lockdrop_immediate_unlock_ratio")]
    pub lockdrop_immediate_unlock_ratio: Decimal256,
    /// The denomination of the LVN token that's used for rewards
    pub lvn_token_denom: String,
}

/// Migrate a farming contract.
#[cw_serde]
pub struct MigrateMsg {}

/// Execute a message on the farming contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Owner messages
    Owner(OwnerExecuteMsg),
    /// Receive entry point for CW20 compatibility
    Receive {
        /// Owner of funds sent to the contract
        sender: RawAddr,
        /// Amount of funds sent
        amount: Uint128,
        /// Must parse to a [ExecuteMsg]
        msg: Binary,
    },
    /// Deposit into a lockdrop bucket
    ///
    /// Valid during the [FarmingStatus::Lockdrop] and [FarmingStatus::Sunset] periods.
    LockdropDeposit {
        /// Which bucket to deposit into
        bucket: LockdropBucket,
    },
    /// Withdraw from a lockdrop bucket
    ///
    /// Valid during the [FarmingStatus::Lockdrop] and [FarmingStatus::Sunset]
    /// periods. During sunset, withdrawals are limited to 50% of the pre-sunset
    /// deposits.
    LockdropWithdraw {
        /// Which bucket to withdraw from
        bucket: LockdropBucket,
        /// Amount of collateral to withdraw
        amount: NonZero<Collateral>,
    },
    /// Deposit into the main farming contract
    ///
    /// Only valid during [FarmingStatus::Launched]
    ///
    /// Note that this supports receiving collateral, LP, or xLP tokens.
    Deposit {},
    /// Withdraw from the main farming contract
    ///
    /// Only valid during [FarmingStatus::Launched]
    ///
    /// In contrast to [ExecuteMsg::Deposit], this will always return xLP tokens.
    Withdraw {
        /// How many farming tokens worth of xLP should we withdraw? If omitted,
        /// withdraws all.
        amount: Option<NonZero<FarmingToken>>,
    },
    /// Claim any pending LVN rewards
    ClaimLvn {},
    /// Claim real yield from the market contract and reinvest as xLP.
    ///
    /// The bonus ratio will be taken off of this first.
    Reinvest {},
    /// Transfer the bonus yield to the bonus wallet.
    TransferBonus {},
}

/// Messages that require owner permissions.
#[cw_serde]
pub enum OwnerExecuteMsg {
    /// Start the lockdrop period
    StartLockdropPeriod {
        /// If specified, lockdrop will start at this time.
        start: Option<Timestamp>,
    },
    /// Finish the review period and launch the primary contract
    StartLaunchPeriod {
        /// If specified, launch will start at this time.
        start: Option<Timestamp>,
    },
    /// Change the active emissions
    SetEmissions {
        /// When to start the emissions.
        ///
        /// If omitted, begins from the timestamp that the block lands.
        start: Option<Timestamp>,
        /// How long the emissions should last, in seconds.
        duration: u32,
        /// How much LVN to deliver
        lvn: NonZero<LvnToken>,
    },
    /// Clear the active emissions
    ClearEmissions {},
    /// Update the configuration set in the [InstantiateMsg]
    ///
    /// Note that, by design, not all fields from [InstantiateMsg] can be
    /// updated, since doing so may violate invariants. Only safely updateable
    /// values are included here.
    UpdateConfig {
        /// See [InstantiateMsg::owner]
        owner: Option<RawAddr>,
        /// The amount of real yield taken for the bonus fund
        bonus_ratio: Option<NonZero<Decimal256>>,
        /// The address that receives bonus transfers.
        bonus_addr: Option<RawAddr>,
    },
}

/// The active emissions plan
#[cw_serde]
pub struct Emissions {
    /// Timestamp that it started
    pub start: Timestamp,
    /// Timestamp that it ends
    pub end: Timestamp,
    /// Total amount of LVN to deliver
    pub lvn: NonZero<LvnToken>,
}

/// Query the farming contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [cw2::ContractVersion]
    #[returns(cw2::ContractVersion)]
    Version {},
    /// Get various information about the state of the contract
    ///
    /// * returns [StatusResp]
    #[returns(StatusResp)]
    Status {},
    /// Get stats on a specific farmer.
    ///
    /// * returns [FarmerStats]
    #[returns(FarmerStats)]
    FarmerStats {
        /// Farmer's address
        addr: RawAddr,
    },
    /// List all farmers known by the contract.
    ///
    /// * returns [FarmersResp]
    #[returns(FarmersResp)]
    Farmers {
        /// Last farmer address seen
        start_after: Option<RawAddr>,
        /// How many to include per batch?
        limit: Option<u32>,
    },
}

/// Overall state of the contract, returned from [QueryMsg::Status]
#[cw_serde]
pub struct StatusResp {
    /// The current farming period, with additional information
    pub period: FarmingPeriodResp,
    /// Total farming tokens across the entire protocol.
    pub farming_tokens: FarmingToken,
    /// Total xLP held by the farming contract
    ///
    /// Note that this number may be different from querying the xLP token
    /// balance since this number won't reflect xLP directly transferred into
    /// this contract.
    pub xlp: LpToken,
    /// The lockdrop bucket information.
    pub lockdrop_buckets: Vec<LockdropBucketStats>,
    /// The amount of collateral in the bonus fund
    pub bonus: Collateral,
    /// If known, the timestamp when all lockdrop LVN rewards are available
    pub lockdrop_rewards_unlocked: Option<Timestamp>,
    /// Total amount of LVN currently held by the contract
    pub lvn_held: LvnToken,
    /// Total liabilities of LVN for the contract.
    ///
    /// This is the sum of unclaimed LVN from lockdrop and emissions, plus any
    /// remaining emissions for the active emissions.
    ///
    /// If this number is less than [StatusResp::lvn_held], the contract is insolvent and needs to be provided with more funds.
    pub lvn_owed: LvnToken,
    /// Current emissions plan
    pub emissions: Option<Emissions>,
}

/// The current farming period, with additional information.
#[cw_serde]
pub enum FarmingPeriodResp {
    /// Contract has been instantiated but lockdrop has not started.
    Inactive {
        /// If set, lockdrop has been scheduled to start at this time
        lockdrop_start: Option<Timestamp>,
    },
    /// Currently in the lockdrop period
    Lockdrop {
        /// Lockdrop started at this time
        started_at: Timestamp,
        /// Sunset will start at this time
        sunset_start: Timestamp,
    },
    /// Currently in the sunset period
    Sunset {
        /// Sunset started at this time
        started_at: Timestamp,
        /// Sunset will end at this time, and manual review period will begin
        review_start: Timestamp,
    },
    /// Sunset completed, waiting for manual review before launching.
    Review {
        /// review started at this time
        started_at: Timestamp,
        /// If set, launch has been scheduled to start at this time
        launch_start: Option<Timestamp>,
    },
    /// Normal contract operations.
    Launched {
        /// Launch started at this time
        started_at: Timestamp,
    },
}

/// A lockdrop bucket, given in number of "months."
///
/// Note that months actually means 30 days, where each day is exactly 24 hours,
/// or 86,400 seconds. (Yes, thanks to leap seconds days can have slightly more
/// than 86,400 seconds.)
#[cw_serde]
#[derive(Copy)]
pub struct LockdropBucket(pub u32);

/// Configuration information for a single lockdrop bucket.
#[cw_serde]
pub struct LockdropBucketConfig {
    /// The bucket duration
    pub bucket: LockdropBucket,
    /// The reward multiplier used
    pub multiplier: NonZero<Decimal256>,
}

/// When a lockdrop bucket will unlock.
#[cw_serde]
pub struct LockdropBucketStats {
    /// The bucket duration
    pub bucket: LockdropBucket,
    /// The reward multiplier used
    pub multiplier: NonZero<Decimal256>,
    /// Total amount deposited
    pub deposit: Collateral,
    /// If we are in the [FarmingStatus::Launched] state, the timestamp when
    /// this will unlock.
    pub unlocks: Option<Timestamp>,
}

/// Stats on a specific farmer
#[cw_serde]
pub struct FarmerStats {
    /// Total farming tokens held
    pub farming_tokens: FarmingToken,
    /// Total farming tokens that can currently be withdrawn
    ///
    /// Tokens from a lockdrop bucket which hasn't unlocked will not be included
    /// here.
    pub farming_tokens_available: FarmingToken,
    /// Information on all lockdrops the farmer is associated with
    pub lockdrops: Vec<FarmerLockdropStats>,
    /// Total lockdrop LVN rewards that are available for claiming
    pub lockdrop_available: LvnToken,
    /// Total lockdrop LVN rewards that are pending unlock
    pub lockdrop_locked: LvnToken,
    /// LVN emissions available for claiming
    pub emissions: LvnToken,
}

/// Information on an individual farmers lockdrop stats.
#[cw_serde]
pub struct FarmerLockdropStats {
    /// The bucket duration
    pub bucket: LockdropBucket,
    /// Total deposit in this bucket
    ///
    /// Note: this number will also always be the same as the number of farming
    /// tokens received for this collateral, since on protocol launch we
    /// guarantee a 1:1 ratio between collateral and farming tokens.
    pub total: Collateral,
    /// Total deposit before the sunset period began
    pub total_before_sunset: Collateral,
    /// Total withdrawals that have occurred during the sunset period
    pub sunset_withdrawals: Collateral,
}

/// Returned from [QueryMsg::Farmers]
#[cw_serde]
pub struct FarmersResp {
    /// The start_after to use in the next query
    pub next_start_after: Option<Addr>,
    /// Addresses found in this batch
    pub farmers: Vec<Addr>,
}
