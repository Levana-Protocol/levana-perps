use std::fmt::Display;

use anyhow::Result;
use cosmos::CosmosNetwork;
use cosmwasm_std::Decimal256;
use shared::storage::{DirectionToBase, Signed};

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
            "omso-test-5" => Ok(ChainId::OsmoTest5),
            "osmosis-1" => Ok(ChainId::Osmosis1),
            "stargaze-1" => Ok(ChainId::Stargaze1),
            "uni-6" => Ok(ChainId::Uni6),
            "pacific-1" => Ok(ChainId::Pacific1),
            _ => Err(anyhow::anyhow!("Unknown chain ID: {value}")),
        }
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
    pub(crate) fn all() -> [ChainId; 9] {
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
        ]
    }

    pub(crate) fn into_cosmos_network(self) -> Result<CosmosNetwork> {
        // In the future this may be a partial mapping (i.e. to None) if we drop
        // support for some chains. But by keeping the ChainId present, we can
        // load historical data from the database.
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
        })
    }

    pub(crate) fn from_cosmos_network(network: CosmosNetwork) -> Result<Self> {
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
}

impl From<PnlType> for String {
    fn from(val: PnlType) -> Self {
        match val {
            PnlType::Usd => "Usd".into(),
            PnlType::Percent => "Percent".into(),
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
