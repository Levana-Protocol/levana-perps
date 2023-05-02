//! Events for the farming contract
use crate::prelude::*;

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
    /// Amount of farming tokens
    pub farming: FarmingToken,
    /// Amount of xLP
    pub xlp: LpToken,
    /// Where did the deposit come from
    pub source: DepositSource,
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
