//! Events for the farming contract
use crate::contracts::farming::entry::LockdropBucketId;
use crate::prelude::*;

/***** Farming Events *****/

/// Event emitted when a new farming contract is instantiated.
pub struct NewFarmingEvent {}

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
    /// The total amount of xLP deposited into the farming contract from all farmers
    pub pool_size: LpToken
}

/// Where did the funds for a farming deposit come from?
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
            pool_size
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
            .add_attribute("pool-size", pool_size.to_string())
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
    /// The total amount of xLP deposited into the farming contract from all farmers
    pub pool_size: LpToken
}

impl From<WithdrawEvent> for Event {
    fn from(
        WithdrawEvent {
            farmer,
            farming,
            xlp,
            pool_size
        }: WithdrawEvent,
    ) -> Self {
        Event::new("withdraw")
            .add_attribute("farmer", farmer)
            .add_attribute("farming", farming.to_string())
            .add_attribute("xlp", xlp.to_string())
            .add_attribute("pool-size", pool_size.to_string())
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

impl From<LockdropLaunchEvent> for Event {
    fn from(src: LockdropLaunchEvent) -> Self {
        Event::new("lockdrop-withdraw-event")
            .add_attribute("launched-at", src.launched_at.to_string())
            .add_attribute("farming-tokens", src.farming_tokens.to_string())
            .add_attribute("xlp", src.xlp.to_string())
    }
}
