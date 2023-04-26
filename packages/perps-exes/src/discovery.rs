//! Code for discovering information on a contract family

use cosmos::{Address, CosmosNetwork};

pub struct ConnectionInfo {
    pub network: CosmosNetwork,
    pub factory_address: Address,
    pub faucet_address: Address,
}

#[derive(serde::Deserialize)]
struct ServerInfo {
    factory: Address,
    faucet: Address,
    network: CosmosNetwork,
}

impl ConnectionInfo {
    pub async fn load(
        client: &reqwest::Client,
        family: &str,
        network_override: Option<CosmosNetwork>,
        factory_override: Option<Address>,
        faucet_override: Option<Address>,
    ) -> anyhow::Result<ConnectionInfo> {
        if let (Some(network), Some(factory), Some(faucet)) =
            (network_override, factory_override, faucet_override)
        {
            return Ok(ConnectionInfo {
                network,
                factory_address: factory,
                faucet_address: faucet,
            });
        }
        let url = format!("https://{}-keeper.sandbox.levana.finance/factory", family);
        let ServerInfo {
            factory,
            faucet,
            network,
        } = client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(ConnectionInfo {
            network: network_override.unwrap_or(network),
            factory_address: factory_override.unwrap_or(factory),
            faucet_address: faucet_override.unwrap_or(faucet),
        })
    }
}
