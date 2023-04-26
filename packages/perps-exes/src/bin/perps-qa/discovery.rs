//! Code for discovering information on a contract family

use cosmos::{Address, CosmosNetwork};

use crate::cli::Opt;

pub(crate) struct ConnectionInfo {
    pub(crate) network: CosmosNetwork,
    pub(crate) factory_address: Address,
    pub(crate) faucet_address: Address,
}

#[derive(serde::Deserialize)]
struct ServerInfo {
    factory: Address,
    faucet: Address,
    network: CosmosNetwork,
}

impl Opt {
    pub(crate) async fn load_connection_info(&self) -> anyhow::Result<ConnectionInfo> {
        let url = format!(
            "https://{}-keeper.sandbox.levana.finance/factory",
            self.contract_family
        );
        let ServerInfo {
            factory,
            faucet,
            network,
        } = reqwest::get(url).await?.error_for_status()?.json().await?;
        Ok(ConnectionInfo {
            network: self.network.unwrap_or(network),
            factory_address: self.factory_contract_address.unwrap_or(factory),
            faucet_address: self.faucet_contract_address.unwrap_or(faucet),
        })
    }
}
