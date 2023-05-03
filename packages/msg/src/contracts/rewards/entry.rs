use crate::contracts::rewards::config::Config;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Decimal256;
use shared::prelude::*;
use shared::time::Timestamp;

/// Instantiate message
#[cw_serde]
pub struct InstantiateMsg {
    /// Configuration
    pub config: ConfigUpdate,
}

/// Execute message
#[cw_serde]
pub enum ExecuteMsg {
    /// Distribute rewards to LPs. A percentage of the rewards will be
    /// transferred to the user immediately. The remainder will unlock linearly over a preconfigured
    /// duration. These values are defined in [Config].
    // FIXME, once integration is done, use IBC receive
    DistributeRewards {
        address: RawAddr,
        /// The total amount of rewards to distribute
        amount: NonZero<LvnToken>,
    },

    /// Update config
    ConfigUpdate { config: ConfigUpdate },

    /// Claim rewards
    Claim {},
}

/// Query message
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// * returns [RewardsInfoResp]
    ///
    /// Rewards information for a given address
    #[returns(RewardsInfoResp)]
    RewardsInfo { addr: RawAddr },

    /// * returns [Config]
    ///
    /// Rewards configuration
    #[returns(Config)]
    Config {},
}

/// Migrate message
#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct RewardsInfoResp {
    /// The amount of tokens locked
    pub locked: Decimal256,
    /// The amount of tokens that are unlocked but have not yet been claimed
    pub unlocked: Decimal256,
    /// The start time of the unlocked period
    pub start: Timestamp,
    /// The end time of the unlocking period
    pub end: Timestamp,
}

impl RewardsInfoResp {
    pub fn new() -> Self {
        RewardsInfoResp {
            locked: Decimal256::zero(),
            unlocked: Decimal256::zero(),
            start: Timestamp::from_seconds(0),
            end: Timestamp::from_seconds(0),
        }
    }
}

impl Default for RewardsInfoResp {
    fn default() -> Self {
        Self::new()
    }
}

#[cw_serde]
pub struct ConfigUpdate {
    pub immediately_transferable: Decimal256,
    pub token_denom: String,
    pub unlock_duration_seconds: u32,
    pub factory_addr: String,
}

pub mod events {
    use crate::constants::event_key;
    use cosmwasm_std::{Addr, Decimal256, Event};
    use shared::prelude::*;

    /// Event when rewards are distributed
    pub struct DistributeRewardsEvent {
        /// The recipient of the rewards
        pub address: Addr,
        /// The amount of tokens
        pub amount: Decimal256,
    }

    impl PerpEvent for DistributeRewardsEvent {}
    impl From<DistributeRewardsEvent> for Event {
        fn from(src: DistributeRewardsEvent) -> Self {
            Event::new(event_key::DISTRIBUTE_REWARDS).add_attributes([
                ("recipient", src.address.to_string()),
                ("amount", src.amount.to_string()),
            ])
        }
    }
    impl TryFrom<Event> for DistributeRewardsEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> Result<Self, Self::Error> {
            Ok(Self {
                address: evt.unchecked_addr_attr(event_key::REWARDS_RECIPIENT)?,
                amount: evt.decimal_attr(event_key::REWARDS_AMOUNT)?,
            })
        }
    }

    /// Event when rewards are claimed
    pub struct ClaimRewardsEvent {
        /// The address of the recipient who is claiming rewards
        pub address: Addr,
        /// The amount of tokens being claimed
        pub amount: Decimal256,
    }

    impl PerpEvent for ClaimRewardsEvent {}
    impl From<ClaimRewardsEvent> for Event {
        fn from(src: ClaimRewardsEvent) -> Self {
            Event::new(event_key::CLAIM_REWARDS).add_attributes([
                ("recipient", src.address.to_string()),
                ("amount", src.amount.to_string()),
            ])
        }
    }
    impl TryFrom<Event> for ClaimRewardsEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> Result<Self, Self::Error> {
            Ok(Self {
                address: evt.unchecked_addr_attr(event_key::REWARDS_RECIPIENT)?,
                amount: evt.decimal_attr(event_key::REWARDS_AMOUNT)?,
            })
        }
    }
}
