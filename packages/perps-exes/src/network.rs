use std::{fmt::Display, str::FromStr};

use cosmos::{error::BuilderError, AddressHrp, CosmosBuilder, CosmosNetwork, HasAddressHrp};

/// Like [CosmosNetwork] but with extra perps-specific networks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum PerpsNetwork {
    Regular(CosmosNetwork),
    DymensionTestnet,
}

impl FromStr for PerpsNetwork {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "dymension-testnet" => PerpsNetwork::DymensionTestnet,
            _ => PerpsNetwork::Regular(s.parse()?),
        })
    }
}

impl PerpsNetwork {
    pub async fn builder(self) -> Result<CosmosBuilder, BuilderError> {
        match self {
            PerpsNetwork::Regular(network) => network.builder().await,
            PerpsNetwork::DymensionTestnet => Ok(CosmosBuilder::new(
                "rollappwasm_1234-2",
                "urax",
                Self::DymensionTestnet.get_address_hrp(),
                "http://18.199.53.161:9090",
            )),
        }
    }
}

impl From<CosmosNetwork> for PerpsNetwork {
    fn from(network: CosmosNetwork) -> Self {
        Self::Regular(network)
    }
}

impl HasAddressHrp for PerpsNetwork {
    fn get_address_hrp(&self) -> AddressHrp {
        match self {
            PerpsNetwork::Regular(network) => network.get_address_hrp(),
            PerpsNetwork::DymensionTestnet => AddressHrp::from_static("rol"),
        }
    }
}

impl serde::Serialize for PerpsNetwork {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PerpsNetwork::Regular(network) => network.serialize(serializer),
            PerpsNetwork::DymensionTestnet => serializer.serialize_str("dymension-testnet"),
        }
    }
}

impl<'de> serde::Deserialize<'de> for PerpsNetwork {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(PerpsNetworkVisitor)
    }
}

struct PerpsNetworkVisitor;

impl<'de> serde::de::Visitor<'de> for PerpsNetworkVisitor {
    type Value = PerpsNetwork;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("PerpsNetwork")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        PerpsNetwork::from_str(v).map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

impl Display for PerpsNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PerpsNetwork::Regular(network) => network.fmt(f),
            PerpsNetwork::DymensionTestnet => f.write_str("dymension-testnet"),
        }
    }
}
