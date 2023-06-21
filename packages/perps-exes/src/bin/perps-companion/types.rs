use cosmos::CosmosNetwork;

/// Chains supported by this server.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Deserialize, sqlx::Type)]
#[repr(i32)]
pub(crate) enum ChainId {
    #[serde(rename = "atlantic-2")]
    Atlantic2 = 1,
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
            _ => Err(anyhow::anyhow!("Unknown chain ID: {value}")),
        }
    }
}

impl TryFrom<String> for ChainId {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl ChainId {
    pub(crate) fn all() -> [ChainId; 8] {
        [
            ChainId::Atlantic2,
            ChainId::Dragonfire4,
            ChainId::Elgafar1,
            ChainId::Juno1,
            ChainId::OsmoTest5,
            ChainId::Osmosis1,
            ChainId::Stargaze1,
            ChainId::Uni6,
        ]
    }

    pub(crate) fn into_cosmos_network(self) -> CosmosNetwork {
        // In the future this may be a partial mapping (i.e. to None) if we drop
        // support for some chains. But by keeping the ChainId present, we can
        // load historical data from the database.
        match self {
            ChainId::Atlantic2 => CosmosNetwork::SeiTestnet,
            ChainId::Dragonfire4 => CosmosNetwork::Dragonfire,
            ChainId::Elgafar1 => CosmosNetwork::StargazeTestnet,
            ChainId::Juno1 => CosmosNetwork::JunoMainnet,
            ChainId::OsmoTest5 => CosmosNetwork::OsmosisTestnet,
            ChainId::Osmosis1 => CosmosNetwork::OsmosisMainnet,
            ChainId::Stargaze1 => CosmosNetwork::StargazeMainnet,
            ChainId::Uni6 => CosmosNetwork::JunoTestnet,
        }
    }
}
