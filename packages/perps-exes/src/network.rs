use std::{fmt::Display, str::FromStr};

use cosmos::{AddressHrp, CosmosBuilder, CosmosConfigError, CosmosNetwork, HasAddressHrp};

/// Like [CosmosNetwork] but with extra perps-specific networks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum PerpsNetwork {
    Regular(CosmosNetwork),
    DymensionTestnet,
    NibiruTestnet,
    RujiraDevnet,
    RujiraTestnet,
    RujiraMainnet,
}

impl FromStr for PerpsNetwork {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "dymension-testnet" => PerpsNetwork::DymensionTestnet,
            "nibiru-testnet" => PerpsNetwork::NibiruTestnet,
            "rujira-devnet" => PerpsNetwork::RujiraDevnet,
            "rujira-testnet" => PerpsNetwork::RujiraTestnet,
            "rujira-mainnet" => PerpsNetwork::RujiraMainnet,
            _ => PerpsNetwork::Regular(s.parse()?),
        })
    }
}

impl PerpsNetwork {
    pub async fn builder(self) -> Result<CosmosBuilder, CosmosConfigError> {
        match self {
            PerpsNetwork::Regular(network) => network.builder_with_config().await,
            PerpsNetwork::DymensionTestnet => Ok(CosmosBuilder::new(
                "rollappwasm_1234-2",
                "urax",
                Self::DymensionTestnet.get_address_hrp(),
                "http://18.199.53.161:9090",
            )),
            PerpsNetwork::NibiruTestnet => Ok(CosmosBuilder::new(
                "nibiru-testnet-1",
                "unibi",
                Self::NibiruTestnet.get_address_hrp(),
                "https://grpc.testnet-1.nibiru.fi",
            )),
            PerpsNetwork::RujiraDevnet => Ok(CosmosBuilder::new(
                "thorchain",
                "rune",
                Self::RujiraDevnet.get_address_hrp(),
                "http://grpc-devnet.starsquid.io:81",
            )),
            PerpsNetwork::RujiraTestnet => Ok(CosmosBuilder::new(
                "thorchain-stagenet-2",
                "rune",
                Self::RujiraTestnet.get_address_hrp(),
                "https://stagenet-grpc.ninerealms.com:443",
            )),
            PerpsNetwork::RujiraMainnet => Ok(CosmosBuilder::new(
                "thorchain-1",
                "rune",
                Self::RujiraMainnet.get_address_hrp(),
                "https://thornode-mainnet-grpc.bryanlabs.net:443",
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
            PerpsNetwork::NibiruTestnet => AddressHrp::from_static("nibi"),
            PerpsNetwork::RujiraDevnet => AddressHrp::from_static("tthor"),
            PerpsNetwork::RujiraTestnet => AddressHrp::from_static("sthor"),
            PerpsNetwork::RujiraMainnet => AddressHrp::from_static("thor"),
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
            PerpsNetwork::NibiruTestnet => serializer.serialize_str("nibiru-testnet"),
            PerpsNetwork::RujiraDevnet => serializer.serialize_str("rujira-devnet"),
            PerpsNetwork::RujiraTestnet => serializer.serialize_str("rujira-testnet"),
            PerpsNetwork::RujiraMainnet => serializer.serialize_str("rujira-mainnet"),
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
            PerpsNetwork::NibiruTestnet => f.write_str("nibiru-testnet"),
            PerpsNetwork::RujiraDevnet => f.write_str("rujira-devnet"),
            PerpsNetwork::RujiraTestnet => f.write_str("rujira-testnet"),
            PerpsNetwork::RujiraMainnet => f.write_str("rujira-mainnet"),
        }
    }
}
