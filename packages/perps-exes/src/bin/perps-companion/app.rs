use std::collections::HashMap;

use anyhow::Result;
use cosmos::{Cosmos, CosmosNetwork};

use crate::cli::Opt;

pub(crate) struct App {
    /// Map from chain ID to a Cosmos connection
    #[allow(dead_code)]
    pub(crate) cosmos: HashMap<String, Cosmos>,
    pub(crate) opt: Opt,
}

impl App {
    pub(crate) fn new(opt: Opt) -> Result<App> {
        Ok(App {
            cosmos: [
                ("atlantic-2", CosmosNetwork::SeiTestnet),
                ("dragonfire-4", CosmosNetwork::Dragonfire),
                ("elgafar-1", CosmosNetwork::StargazeTestnet),
                ("juno-1", CosmosNetwork::JunoMainnet),
                ("osmo-test-5", CosmosNetwork::OsmosisTestnet),
                ("osmosis-1", CosmosNetwork::OsmosisMainnet),
                ("stargaze-1", CosmosNetwork::StargazeMainnet),
                ("uni-6", CosmosNetwork::JunoTestnet),
            ]
            .into_iter()
            .map(|(chain_id, network)| (chain_id.to_owned(), network.builder().build_lazy()))
            .collect(),
            opt,
        })
    }
}
