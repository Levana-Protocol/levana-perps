// The types must be represented with repr statement which is using as conversion internally.
#![allow(clippy::as_conversions)]

use std::{fmt::Display, str::FromStr};

use anyhow::Result;
use cosmos::{CosmosBuilder, CosmosNetwork};
use cosmwasm_std::Decimal256;
use perps_exes::PerpsNetwork;
use perpswap::storage::{DirectionToBase, Signed};

/// Chains supported by this server.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Deserialize, sqlx::Type)]
#[repr(i32)]
pub(crate) enum ChainId {
    #[serde(rename = "atlantic-2")]
    Atlantic2 = 1,
    // Leaving in place for backwards compat in a few places, but not allowing
    // new positions to be stored.
    #[serde(rename = "dragonfire-4")]
    Dragonfire4 = 2,
    #[serde(rename = "elgafar-1")]
    Elgafar1 = 3,
    #[serde(rename = "juno-1")]
    Juno1 = 4,
    #[serde(rename = "osmo-test-5")]
    OsmoTest5 = 5,
    #[serde(rename = "osmosis-1")]
    Osmosis1 = 6,
    #[serde(rename = "stargaze-1")]
    Stargaze1 = 7,
    #[serde(rename = "uni-6")]
    Uni6 = 8,
    #[serde(rename = "pacific-1")]
    Pacific1 = 9,
    #[serde(rename = "injective-1")]
    Injective1 = 10,
    #[serde(rename = "injective-888")]
    Injective888 = 11,
    #[serde(rename = "neutron-1")]
    Neutron1 = 12,
    #[serde(rename = "pion-1")]
    Pion1 = 13,
    #[serde(rename = "thorchain-stagenet-2")]
    RujiraTestnet = 14,
    #[serde(rename = "thorchain-1")]
    RujiraMainnet = 15,
}

impl From<ChainId> for i32 {
    fn from(value: ChainId) -> Self {
        match value {
            ChainId::Atlantic2 => 1,
            ChainId::Dragonfire4 => 2,
            ChainId::Elgafar1 => 3,
            ChainId::Juno1 => 4,
            ChainId::OsmoTest5 => 5,
            ChainId::Osmosis1 => 6,
            ChainId::Stargaze1 => 7,
            ChainId::Uni6 => 8,
            ChainId::Pacific1 => 9,
            ChainId::Injective1 => 10,
            ChainId::Injective888 => 11,
            ChainId::Neutron1 => 12,
            ChainId::Pion1 => 13,
            ChainId::RujiraTestnet => 14,
            ChainId::RujiraMainnet => 15,
        }
    }
}

impl TryFrom<&str> for ChainId {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // Annoying that we're repeating the values above...
        match value {
            "atlantic-2" => Ok(ChainId::Atlantic2),
            "dragonfire-4" => Ok(ChainId::Dragonfire4),
            "elgafar-1" => Ok(ChainId::Elgafar1),
            "juno-1" => Ok(ChainId::Juno1),
            "osmo-test-5" => Ok(ChainId::OsmoTest5),
            "osmosis-1" => Ok(ChainId::Osmosis1),
            "stargaze-1" => Ok(ChainId::Stargaze1),
            "uni-6" => Ok(ChainId::Uni6),
            "pacific-1" => Ok(ChainId::Pacific1),
            "injective-1" => Ok(ChainId::Injective1),
            "injective-888" => Ok(ChainId::Injective888),
            "neutron-1" => Ok(ChainId::Neutron1),
            "pion-1" => Ok(ChainId::Pion1),
            "thorchain-stagenet-2" => Ok(ChainId::RujiraTestnet),
            "thorchain-1" => Ok(ChainId::RujiraMainnet),
            _ => Err(anyhow::anyhow!("Unknown chain ID: {value}")),
        }
    }
}

impl FromStr for ChainId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        s.try_into()
    }
}

// And more duplication! Doh!
impl Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            ChainId::Atlantic2 => "atlantic-2",
            ChainId::Dragonfire4 => "dragonfire-4",
            ChainId::Elgafar1 => "elgafar-1",
            ChainId::Juno1 => "juno-1",
            ChainId::OsmoTest5 => "osmo-test-5",
            ChainId::Osmosis1 => "osmosis-1",
            ChainId::Stargaze1 => "stargaze-1",
            ChainId::Uni6 => "uni-6",
            ChainId::Pacific1 => "pacific-1",
            ChainId::Injective1 => "injective-1",
            ChainId::Injective888 => "injective-888",
            ChainId::Neutron1 => "neutron-1",
            ChainId::Pion1 => "pion-1",
            ChainId::RujiraTestnet => "thorchain-stagenet-2",
            ChainId::RujiraMainnet => "thorchain-1",
        })
    }
}

impl TryFrom<String> for ChainId {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl ChainId {
    pub(crate) fn all() -> [ChainId; 14] {
        [
            ChainId::Atlantic2,
            ChainId::Elgafar1,
            ChainId::Juno1,
            ChainId::OsmoTest5,
            ChainId::Osmosis1,
            ChainId::Stargaze1,
            ChainId::Uni6,
            ChainId::Pacific1,
            ChainId::Injective1,
            ChainId::Injective888,
            ChainId::Neutron1,
            ChainId::Pion1,
            ChainId::RujiraTestnet,
            ChainId::RujiraMainnet,
        ]
    }

    pub(crate) async fn into_cosmos_builder(self) -> Result<CosmosBuilder> {
        // In the future this may be a partial mapping (i.e. to None) if we drop
        // support for some chains. But by keeping the ChainId present, we can
        // load historical data from the database.
        if self == ChainId::RujiraTestnet {
            return Ok(PerpsNetwork::RujiraTestnet.builder().await?);
        }

        Ok(match self {
            ChainId::Atlantic2 => CosmosNetwork::SeiTestnet,
            ChainId::Dragonfire4 => anyhow::bail!("Dragonfire network is no longer supported"),
            ChainId::Elgafar1 => CosmosNetwork::StargazeTestnet,
            ChainId::Juno1 => CosmosNetwork::JunoMainnet,
            ChainId::OsmoTest5 => CosmosNetwork::OsmosisTestnet,
            ChainId::Osmosis1 => CosmosNetwork::OsmosisMainnet,
            ChainId::Stargaze1 => CosmosNetwork::StargazeMainnet,
            ChainId::Uni6 => CosmosNetwork::JunoTestnet,
            ChainId::Pacific1 => CosmosNetwork::SeiMainnet,
            ChainId::Injective1 => CosmosNetwork::InjectiveMainnet,
            ChainId::Injective888 => CosmosNetwork::InjectiveTestnet,
            ChainId::Neutron1 => CosmosNetwork::NeutronMainnet,
            ChainId::Pion1 => CosmosNetwork::NeutronTestnet,
            _ => anyhow::bail!("Unsupported Chain Id"),
        }
        .builder()
        .await?)
    }

    pub(crate) fn from_perps_network(network: PerpsNetwork) -> Result<Self> {
        match network {
            PerpsNetwork::Regular(network) => Self::from_cosmos_network(network),
            PerpsNetwork::RujiraTestnet => Ok(ChainId::RujiraTestnet),
            PerpsNetwork::RujiraMainnet => Ok(ChainId::RujiraMainnet),
            PerpsNetwork::DymensionTestnet => Err(anyhow::anyhow!(
                "Cannot run companion server for Dymension testnet"
            )),
            PerpsNetwork::NibiruTestnet => Err(anyhow::anyhow!(
                "Cannot run companion server for Nibiru testnet"
            )),
        }
    }

    fn from_cosmos_network(network: CosmosNetwork) -> Result<Self> {
        match network {
            CosmosNetwork::JunoTestnet => Ok(ChainId::Uni6),
            CosmosNetwork::JunoMainnet => Ok(ChainId::Juno1),
            CosmosNetwork::OsmosisMainnet => Ok(ChainId::Osmosis1),
            CosmosNetwork::OsmosisTestnet => Ok(ChainId::OsmoTest5),
            CosmosNetwork::SeiMainnet => Ok(ChainId::Pacific1),
            CosmosNetwork::SeiTestnet => Ok(ChainId::Atlantic2),
            CosmosNetwork::StargazeTestnet => Ok(ChainId::Elgafar1),
            CosmosNetwork::StargazeMainnet => Ok(ChainId::Stargaze1),
            CosmosNetwork::InjectiveMainnet => Ok(ChainId::Injective1),
            CosmosNetwork::InjectiveTestnet => Ok(ChainId::Injective888),
            CosmosNetwork::NeutronMainnet => Ok(ChainId::Neutron1),
            CosmosNetwork::NeutronTestnet => Ok(ChainId::Pion1),
            _ => Err(anyhow::anyhow!("Unsupported network: {network}")),
        }
    }

    pub(crate) fn is_mainnet(self) -> bool {
        match self {
            ChainId::Atlantic2 => false,
            ChainId::Dragonfire4 => false,
            ChainId::Elgafar1 => false,
            ChainId::Juno1 => true,
            ChainId::OsmoTest5 => false,
            ChainId::Osmosis1 => true,
            ChainId::Stargaze1 => true,
            ChainId::Uni6 => false,
            ChainId::Pacific1 => true,
            ChainId::Injective1 => true,
            ChainId::Injective888 => false,
            ChainId::Neutron1 => true,
            ChainId::Pion1 => false,
            ChainId::RujiraTestnet => false,
            ChainId::RujiraMainnet => true,
        }
    }
}

/// Which analytics environment the contract is part of
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, sqlx::Type)]
#[repr(i32)]
pub(crate) enum ContractEnvironment {
    Mainnet = 1,
    Beta = 2,
    Dev = 3,
}

impl From<ContractEnvironment> for i32 {
    fn from(value: ContractEnvironment) -> Self {
        match value {
            ContractEnvironment::Mainnet => 1,
            ContractEnvironment::Beta => 2,
            ContractEnvironment::Dev => 3,
        }
    }
}

impl ContractEnvironment {
    pub(crate) fn amplitude_key(self) -> &'static str {
        match self {
            ContractEnvironment::Mainnet => "b95d602af8198e98fb113a4e01b02ac7",
            ContractEnvironment::Beta => "90522542888df13ac43bc467698fa94d",
            ContractEnvironment::Dev => "272aaf66576c3fe4d054149073bb70a2",
        }
    }

    pub(crate) fn from_market(chain: ChainId, label: &str) -> Self {
        if chain.is_mainnet() {
            ContractEnvironment::Mainnet
        } else if is_beta(label) {
            ContractEnvironment::Beta
        } else {
            ContractEnvironment::Dev
        }
    }
}

fn is_beta(label: &str) -> bool {
    label.ends_with("beta") || label.ends_with("trade")
}

/// Is this is a long or short position
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, sqlx::Type)]
#[repr(i32)]
pub(crate) enum DirectionForDb {
    Long = 1,
    Short = 2,
}

impl From<DirectionForDb> for i32 {
    fn from(value: DirectionForDb) -> Self {
        match value {
            DirectionForDb::Long => 1,
            DirectionForDb::Short => 2,
        }
    }
}

impl From<DirectionToBase> for DirectionForDb {
    fn from(src: DirectionToBase) -> Self {
        match src {
            DirectionToBase::Long => Self::Long,
            DirectionToBase::Short => Self::Short,
        }
    }
}

impl Display for DirectionForDb {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            DirectionForDb::Long => "LONG",
            DirectionForDb::Short => "SHORT",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[repr(i32)]
pub(crate) enum PnlType {
    Usd = 1,
    Percent = 2,
    Both = 3,
}

impl From<PnlType> for String {
    fn from(val: PnlType) -> Self {
        match val {
            PnlType::Usd => "Usd".into(),
            PnlType::Percent => "Percent".into(),
            PnlType::Both => "Both".into(),
        }
    }
}

impl From<PnlType> for i32 {
    fn from(value: PnlType) -> Self {
        match value {
            PnlType::Usd => 1,
            PnlType::Percent => 2,
            PnlType::Both => 3,
        }
    }
}

pub(crate) struct TwoDecimalPoints(pub(crate) Signed<Decimal256>);

impl Display for TwoDecimalPoints {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let ten = Decimal256::from_ratio(10u32, 1u32);
        let half = Decimal256::from_ratio(1u32, 2u32);

        if self.0.is_negative() {
            write!(f, "-")?;
        }

        let whole = self.0.abs_unsigned().floor();
        let rem = self.0.abs_unsigned() - whole;
        let rem = rem * ten;
        let x = rem.floor();
        let rem = rem - x;
        let rem = rem * ten;
        let y = rem.floor();
        let rem = rem - y;
        let y = if rem >= half {
            y + Decimal256::one()
        } else {
            y
        };
        write!(f, "{}.{}{}", whole, x, y)
    }
}

/// Check if a given chain ID is for Rujira.
pub(crate) fn is_rujira_chain(chain_id: &str) -> bool {
    match chain_id.parse::<ChainId>() {
        // If it's an invalid chain ID, treat it as non-Rujira
        Err(_) => false,
        Ok(ChainId::RujiraTestnet | ChainId::RujiraMainnet) => true,
        // Keep an explicit list here so that, when adding new chains,
        // the compiler forces us to decide if the chain should use
        // Rujira styling or not.
        Ok(
            ChainId::Atlantic2
            | ChainId::Dragonfire4
            | ChainId::Elgafar1
            | ChainId::Juno1
            | ChainId::OsmoTest5
            | ChainId::Osmosis1
            | ChainId::Stargaze1
            | ChainId::Uni6
            | ChainId::Pacific1
            | ChainId::Injective1
            | ChainId::Injective888
            | ChainId::Neutron1
            | ChainId::Pion1,
        ) => false,
    }
}
