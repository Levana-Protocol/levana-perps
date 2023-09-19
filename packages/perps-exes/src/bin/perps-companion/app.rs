use std::collections::HashMap;

use anyhow::{Context, Result};
use cosmos::{Cosmos, CosmosNetwork};
use perps_exes::{config::MainnetFactories, contracts::Factory};

use crate::{cli::Opt, db::handle::Db, types::ChainId};

pub(crate) struct App {
    /// Map from chain ID to a Cosmos connection
    #[allow(dead_code)]
    pub(crate) cosmos: HashMap<ChainId, Cosmos>,
    pub(crate) opt: Opt,
    pub(crate) db: Db,
    pub(crate) factories: Vec<(Factory, CosmosNetwork)>,
    pub(crate) client: reqwest::Client,
}

impl App {
    pub(crate) async fn new(opt: Opt) -> Result<App> {
        let postgres_uri = opt.pgopt.uri();
        let db = Db::new(&postgres_uri).await?;
        let mut cosmos_map = HashMap::new();
        for chain_id in ChainId::all() {
            let cosmos = chain_id
                .into_cosmos_network()?
                .builder()
                .await?
                .build_lazy();
            cosmos_map.insert(chain_id, cosmos);
        }

        let factories = MainnetFactories::load_hard_coded()?
            .factories
            .into_iter()
            .filter(|x| x.canonical)
            .map(|factory| {
                let chain_id = ChainId::from_cosmos_network(factory.network)?;
                let cosmos = cosmos_map
                    .get(&chain_id)
                    .with_context(|| format!("No Cosmos client found for {chain_id}"))?;
                Ok((
                    Factory::from_contract(cosmos.make_contract(factory.address)),
                    factory.network,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let client = reqwest::ClientBuilder::new()
            .user_agent("Companion server")
            .build()?;

        Ok(App {
            cosmos: cosmos_map,
            opt,
            db,
            factories,
            client,
        })
    }

    pub(crate) async fn migrate_db(&self) -> Result<()> {
        sqlx::migrate!("src/bin/perps-companion/migrations")
            .run(&self.db.pool)
            .await
            .context("Error while running database migrations")
    }
}
