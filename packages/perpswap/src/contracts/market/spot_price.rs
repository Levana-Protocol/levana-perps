//! Spot price data structures

use crate::storage::{NumberGtZero, RawAddr};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use pyth_sdk_cw::PriceIdentifier;
use std::str::FromStr;

/// Spot price config
#[cw_serde]
pub enum SpotPriceConfig {
    /// Manual spot price
    Manual {
        /// The admin address for manual spot price updates
        admin: Addr,
    },
    /// External oracle
    Oracle {
        /// Pyth configuration, required on chains that use pyth feeds
        pyth: Option<PythConfig>,
        /// Stride configuration, required on chains that use stride
        stride: Option<StrideConfig>,
        /// sequence of spot price feeds which are composed to generate a single spot price
        feeds: Vec<SpotPriceFeed>,
        /// if necessary, sequence of spot price feeds which are composed to generate a single USD spot price
        feeds_usd: Vec<SpotPriceFeed>,
        /// How many seconds the publish time of volatile feeds are allowed to diverge from each other
        ///
        /// An attacker can, in theory, selectively choose two different publish
        /// times for a pair of assets and manipulate the combined price. This value allows
        /// us to say that the publish time cannot diverge by too much. As opposed to age
        /// tolerance, this allows for latency in getting transactions to land on-chain
        /// after publish time, and therefore can be a much tighter value.
        ///
        /// By default, we use 5 seconds.
        volatile_diff_seconds: Option<u32>,
    },
}

/// Configuration for pyth
#[cw_serde]
pub struct PythConfig {
    /// The address of the pyth oracle contract
    pub contract_address: Addr,
    /// Which network to use for the price service
    /// This isn't used for any internal logic, but clients must use the appropriate
    /// price service endpoint to match this
    pub network: PythPriceServiceNetwork,
}

/// Configuration for stride
#[cw_serde]
pub struct StrideConfig {
    /// The address of the redemption rate contract
    pub contract_address: Addr,
}

/// An individual feed used to compose a final spot price
#[cw_serde]
pub struct SpotPriceFeed {
    /// The data for this price feed
    pub data: SpotPriceFeedData,
    /// is this price feed inverted
    pub inverted: bool,
    /// Is this a volatile feed?
    ///
    /// Volatile feeds are expected to have frequent and significant price
    /// swings. By contrast, a non-volatile feed may be a redemption rate, which will
    /// slowly update over time. The purpose of volatility is to determine whether
    /// the publich time for a composite spot price should include the individual feed
    /// or not. For example, if we have a market like StakedETH_BTC, we would have a
    /// StakedETH redemption rate, the price of ETH, and the price of BTC. We'd mark ETH
    /// and BTC as volatile, and the redemption rate as non-volatile. Then the publish
    /// time would be the earlier of the ETH and BTC publish time.
    ///
    /// This field is optional. If omitted, it will use a default based on
    /// the `data` field, specifically: Pyth and Sei variants are considered volatile,
    /// Constant, Stride, and Simple are non-volatile.
    pub volatile: Option<bool>,
}

/// The data for an individual spot price feed
#[cw_serde]
pub enum SpotPriceFeedData {
    /// Hardcoded value
    Constant {
        /// The constant price
        price: NumberGtZero,
    },
    /// Pyth price feeds
    Pyth {
        /// The identifier on pyth
        id: PriceIdentifier,
        /// price age tolerance, in seconds
        ///
        /// We thought about removing this parameter when moving to deferred
        /// execution. However, this would leave open a potential attack vector of opening
        /// limit orders or positions, shutting down price updates, and then selectively
        /// replaying old price updates for favorable triggers.
        age_tolerance_seconds: u32,
    },
    /// Stride liquid staking
    Stride {
        /// The IBC denom for the asset
        denom: String,
        /// price age tolerance, in seconds
        age_tolerance_seconds: u32,
    },
    /// Native oracle module on the sei chain
    Sei {
        /// The denom to use
        denom: String,
    },
    /// Simple contract with a QueryMsg::Price call
    Simple {
        /// The contract to use
        contract: Addr,
        /// price age tolerance, in seconds
        age_tolerance_seconds: u32,
    },
}

/// Which network to use for the price service
#[cw_serde]
#[derive(Copy)]
pub enum PythPriceServiceNetwork {
    /// Stable CosmWasm
    ///
    /// From <https://pyth.network/developers/price-feed-ids#cosmwasm-stable>
    Stable,
    /// Edge CosmWasm
    ///
    /// From <https://pyth.network/developers/price-feed-ids#cosmwasm-edge>
    Edge,
}

impl FromStr for PythPriceServiceNetwork {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "stable" => Ok(Self::Stable),
            "edge" => Ok(Self::Edge),
            _ => Err(anyhow::anyhow!(
                "Invalid feed type: {s}. Expected 'stable' or 'edge'"
            )),
        }
    }
}

/********* Just for config init *********/
/// Spot price config for initialization messages
#[cw_serde]
pub enum SpotPriceConfigInit {
    /// Manual spot price
    Manual {
        /// The admin address for manual spot price updates
        admin: RawAddr,
    },
    /// External oracle
    Oracle {
        /// Pyth configuration, required on chains that use pyth feeds
        pyth: Option<PythConfigInit>,
        /// Stride configuration, required on chains that use stride feeds
        stride: Option<StrideConfigInit>,
        /// sequence of spot price feeds which are composed to generate a single spot price
        feeds: Vec<SpotPriceFeedInit>,
        /// if necessary, sequence of spot price feeds which are composed to generate a single USD spot price
        feeds_usd: Vec<SpotPriceFeedInit>,
        /// See [SpotPriceConfig::Oracle::volatile_diff_seconds]
        volatile_diff_seconds: Option<u32>,
    },
}

impl From<SpotPriceConfig> for SpotPriceConfigInit {
    fn from(src: SpotPriceConfig) -> Self {
        match src {
            SpotPriceConfig::Manual { admin } => Self::Manual {
                admin: RawAddr::from(admin),
            },
            SpotPriceConfig::Oracle {
                pyth,
                stride,
                feeds,
                feeds_usd,
                volatile_diff_seconds,
            } => Self::Oracle {
                pyth: pyth.map(|pyth| PythConfigInit {
                    contract_address: RawAddr::from(pyth.contract_address),
                    network: pyth.network,
                }),
                stride: stride.map(|stride| StrideConfigInit {
                    contract_address: RawAddr::from(stride.contract_address),
                }),
                feeds: feeds.iter().map(|feed| feed.clone().into()).collect(),
                feeds_usd: feeds_usd
                    .iter()
                    .map(|feed_usd| feed_usd.clone().into())
                    .collect(),
                volatile_diff_seconds,
            },
        }
    }
}

/// An individual feed used to compose a final spot price
#[cw_serde]
pub struct SpotPriceFeedInit {
    /// The data for this price feed
    pub data: SpotPriceFeedDataInit,
    /// is this price feed inverted
    pub inverted: bool,
    /// See [SpotPriceFeed::volatile]
    pub volatile: Option<bool>,
}
impl From<SpotPriceFeed> for SpotPriceFeedInit {
    fn from(src: SpotPriceFeed) -> Self {
        Self {
            data: src.data.into(),
            inverted: src.inverted,
            volatile: src.volatile,
        }
    }
}

/// The data for an individual spot price feed
#[cw_serde]
pub enum SpotPriceFeedDataInit {
    /// Hardcoded value
    Constant {
        /// The constant price
        price: NumberGtZero,
    },
    /// Pyth price feeds
    Pyth {
        /// The identifier on pyth
        id: PriceIdentifier,
        /// price age tolerance, in seconds
        age_tolerance_seconds: u32,
    },
    /// Stride liquid staking
    Stride {
        /// The IBC denom for the asset
        denom: String,
        /// price age tolerance, in seconds
        age_tolerance_seconds: u32,
    },
    /// Native oracle module on the sei chain
    Sei {
        /// The denom to use
        denom: String,
    },
    /// Simple contract with a QueryMsg::Price call
    Simple {
        /// The contract to use
        contract: RawAddr,
        /// price age tolerance, in seconds
        age_tolerance_seconds: u32,
    },
}
impl From<SpotPriceFeedData> for SpotPriceFeedDataInit {
    fn from(src: SpotPriceFeedData) -> Self {
        match src {
            SpotPriceFeedData::Constant { price } => SpotPriceFeedDataInit::Constant { price },
            SpotPriceFeedData::Pyth {
                id,
                age_tolerance_seconds,
            } => SpotPriceFeedDataInit::Pyth {
                id,
                age_tolerance_seconds,
            },
            SpotPriceFeedData::Stride {
                denom,
                age_tolerance_seconds,
            } => SpotPriceFeedDataInit::Stride {
                denom,
                age_tolerance_seconds,
            },
            SpotPriceFeedData::Sei { denom } => SpotPriceFeedDataInit::Sei { denom },
            SpotPriceFeedData::Simple {
                contract,
                age_tolerance_seconds,
            } => SpotPriceFeedDataInit::Simple {
                contract: contract.into(),
                age_tolerance_seconds,
            },
        }
    }
}

/// Configuration for pyth init messages
#[cw_serde]
pub struct PythConfigInit {
    /// The address of the pyth oracle contract
    pub contract_address: RawAddr,
    /// Which network to use for the price service
    /// This isn't used for any internal logic, but clients must use the appropriate
    /// price service endpoint to match this
    pub network: PythPriceServiceNetwork,
}

/// Configuration for stride
#[cw_serde]
pub struct StrideConfigInit {
    /// The address of the redemption rate contract
    pub contract_address: RawAddr,
}

/// Spot price events
pub mod events {
    use crate::prelude::*;
    use cosmwasm_std::Event;

    /// Event emited when a new spot price is added to the protocol.
    pub struct SpotPriceEvent {
        /// Timestamp of the update
        pub timestamp: Timestamp,
        /// Price of the collateral asset in USD
        pub price_usd: PriceCollateralInUsd,
        /// Price of the notional asset in collateral, generated by the protocol
        pub price_notional: Price,
        /// Price of the base asset in quote
        pub price_base: PriceBaseInQuote,
        /// publish time, if available
        pub publish_time: Option<Timestamp>,
        /// publish time, if available
        pub publish_time_usd: Option<Timestamp>,
    }

    impl From<SpotPriceEvent> for Event {
        fn from(src: SpotPriceEvent) -> Self {
            let mut evt = Event::new("spot-price").add_attributes(vec![
                ("price-usd", src.price_usd.to_string()),
                ("price-notional", src.price_notional.to_string()),
                ("price-base", src.price_base.to_string()),
                ("time", src.timestamp.to_string()),
            ]);

            if let Some(publish_time) = src.publish_time {
                evt = evt.add_attribute("publish-time", publish_time.to_string());
            }
            if let Some(publish_time_usd) = src.publish_time_usd {
                evt = evt.add_attribute("publish-time-usd", publish_time_usd.to_string());
            }

            evt
        }
    }
    impl TryFrom<Event> for SpotPriceEvent {
        type Error = anyhow::Error;

        fn try_from(evt: Event) -> anyhow::Result<Self> {
            Ok(Self {
                timestamp: evt.timestamp_attr("time")?,
                price_usd: PriceCollateralInUsd::try_from_number(evt.number_attr("price-usd")?)?,
                price_notional: Price::try_from_number(evt.number_attr("price-notional")?)?,
                price_base: PriceBaseInQuote::try_from_number(evt.number_attr("price-base")?)?,
                publish_time: evt.try_timestamp_attr("publish-time")?,
                publish_time_usd: evt.try_timestamp_attr("publish-time-usd")?,
            })
        }
    }
}
