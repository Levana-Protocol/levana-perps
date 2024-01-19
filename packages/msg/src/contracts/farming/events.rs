//! Events for the farming contract
use crate::contracts::farming::entry::LockdropBucketId;
use crate::prelude::*;

/***** Farming Events *****/

/// Event emitted when a new farming contract is instantiated.
pub struct NewFarmingEvent {}
impl PerpEvent for NewFarmingEvent {}

impl From<NewFarmingEvent> for Event {
    fn from(NewFarmingEvent {}: NewFarmingEvent) -> Self {
        Event::new("levana-new-farming")
    }
}

/// xLP was deposited into the contract
pub struct DepositEvent {
    /// Farmer
    pub farmer: Addr,
    /// Amount of farming tokens minted to the farmer
    pub farming: FarmingToken,
    /// Amount of xLP deposited
    pub xlp: LpToken,
    /// The asset originally deposited by the farmer
    pub source: DepositSource,
}
impl PerpEvent for DepositEvent {}

/// Where did the funds for a farming deposit come from?
#[cw_serde]
pub enum DepositSource {
    /// Farmer deposited collateral
    Collateral,
    /// Farmer deposited LP tokens
    Lp,
    /// Farmer deposited xLP tokens
    Xlp,
}

impl From<DepositEvent> for Event {
    fn from(
        DepositEvent {
            farmer,
            farming,
            xlp,
            source,
        }: DepositEvent,
    ) -> Self {
        Event::new("deposit")
            .add_attribute("farmer", farmer)
            .add_attribute("farming", farming.to_string())
            .add_attribute("xlp", xlp.to_string())
            .add_attribute(
                "source",
                match source {
                    DepositSource::Collateral => "collateral",
                    DepositSource::Lp => "lp",
                    DepositSource::Xlp => "xlp",
                },
            )
    }
}

/// xLP was withdrawn from the contract
pub struct WithdrawEvent {
    /// Farmer
    pub farmer: Addr,
    /// Amount of farming tokens
    pub farming: FarmingToken,
    /// Amount of xLP
    pub xlp: LpToken,
}
impl PerpEvent for WithdrawEvent {}

impl From<WithdrawEvent> for Event {
    fn from(
        WithdrawEvent {
            farmer,
            farming,
            xlp,
        }: WithdrawEvent,
    ) -> Self {
        Event::new("withdraw")
            .add_attribute("farmer", farmer)
            .add_attribute("farming", farming.to_string())
            .add_attribute("xlp", xlp.to_string())
    }
}

/***** Lockdrop Events *****/

/// Collateral was deposited into the lockdrop
pub struct LockdropDepositEvent {
    /// The address of the farmer depositing
    pub farmer: Addr,
    /// The amount deposited
    pub amount: Collateral,
    /// The bucket into which `amount` is being deposited
    pub bucket_id: LockdropBucketId,
}
impl PerpEvent for LockdropDepositEvent {}

impl From<LockdropDepositEvent> for Event {
    fn from(src: LockdropDepositEvent) -> Self {
        Event::new("lockdrop-deposit-event")
            .add_attribute("farmer", src.farmer)
            .add_attribute("amount", src.amount.to_string())
            .add_attribute("bucket_id", src.bucket_id.to_string())
    }
}

/// Collateral was withdrawn from the lockdrop
pub struct LockdropWithdrawEvent {
    /// The address of the farmer depositing
    pub farmer: Addr,
    /// The amount deposited
    pub amount: Collateral,
    /// The bucket into which `amount` is being deposited
    pub bucket_id: LockdropBucketId,
}
impl PerpEvent for LockdropWithdrawEvent {}

impl From<LockdropWithdrawEvent> for Event {
    fn from(src: LockdropWithdrawEvent) -> Self {
        Event::new("lockdrop-withdraw-event")
            .add_attribute("farmer", src.farmer)
            .add_attribute("amount", src.amount.to_string())
            .add_attribute("bucket_id", src.bucket_id.to_string())
    }
}

/// The lockdrop was launched (i.e. [FarmingPeriod::Launched])
pub struct LockdropLaunchEvent {
    /// The time the lockdrop launched
    pub launched_at: Timestamp,
    /// The amount of farming tokens minted during the lockdrop
    pub farming_tokens: FarmingToken,
    /// The amount of xlp tokens minted during launch
    pub xlp: LpToken,
}
impl PerpEvent for LockdropLaunchEvent {}

impl From<LockdropLaunchEvent> for Event {
    fn from(src: LockdropLaunchEvent) -> Self {
        Event::new("lockdrop-launch-event")
            .add_attribute("launched-at", src.launched_at.to_string())
            .add_attribute("farming-tokens", src.farming_tokens.to_string())
            .add_attribute("xlp", src.xlp.to_string())
    }
}

/// Accrued market yield was reinvested
pub struct ReinvestEvent {
    /// The amount of yield accrued
    pub reinvested_yield: Collateral,
    /// The amount of new xLP
    pub xlp: LpToken,
    /// The amount of yield allocated to the bonus fund
    pub bonus_yield: Collateral,
}
impl PerpEvent for ReinvestEvent {}

impl From<ReinvestEvent> for Event {
    fn from(src: ReinvestEvent) -> Self {
        Event::new("reinvest-event")
            .add_attribute("reinvested_yield", src.reinvested_yield.to_string())
            .add_attribute("xlp", src.xlp.to_string())
            .add_attribute("bonus_yield", src.bonus_yield.to_string())
    }
}

/// New emissions are set
pub struct SetEmissionsEvent {
    /// The emissions start time
    pub start: Timestamp,
    /// The duration of the emissions
    pub duration: u32,
    /// The amount of LVN tokens
    pub tokens: LvnToken,
}

impl From<SetEmissionsEvent> for Event {
    fn from(src: SetEmissionsEvent) -> Self {
        Event::new("set-emissions-event")
            .add_attribute("start", src.start.to_string())
            .add_attribute("duration", src.duration.to_string())
            .add_attribute("tokens", src.tokens.to_string())
    }
}

/// Emissions are cleared
pub struct ClearEmissionsEvent {
    /// The timestamp for when the emissions were cleared
    pub cleared_at: Timestamp,
    /// The amount of LVN tokens that are no longer being distributed
    pub remaining_lvn: LvnToken,
}

impl From<ClearEmissionsEvent> for Event {
    fn from(src: ClearEmissionsEvent) -> Self {
        Event::new("clear-emissions-event")
            .add_attribute("cleared-at", src.cleared_at.to_string())
            .add_attribute("remaining-lvn", src.remaining_lvn.to_string())
    }
}

/// Provides current size of the farming assets
pub struct FarmingPoolSizeEvent {
    /// Total amount of minted farming tokens
    pub farming: FarmingToken,
    /// Total amount of xLP held by the farming contract
    pub xlp: LpToken,
}
impl PerpEvent for FarmingPoolSizeEvent {}

impl From<FarmingPoolSizeEvent> for Event {
    fn from(src: FarmingPoolSizeEvent) -> Self {
        Event::new("farming-pool-size-event")
            .add_attribute("farming", src.farming.to_string())
            .add_attribute("xlp", src.xlp.to_string())
    }
}
