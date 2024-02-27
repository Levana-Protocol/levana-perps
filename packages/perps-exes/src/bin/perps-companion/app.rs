use std::collections::HashMap;

use anyhow::{Context, Result};
use cosmos::{Cosmos, CosmosNetwork};
use perps_exes::{config::MainnetFactories, contracts::Factory};
use resvg::usvg::fontdb::Database;

use crate::{cli::Opt, db::handle::Db, types::ChainId};

pub(crate) struct App {
    /// Map from chain ID to a Cosmos connection
    pub(crate) cosmos: HashMap<ChainId, Cosmos>,
    pub(crate) opt: Opt,
    pub(crate) db: Db,
    pub(crate) factories: Vec<(Factory, CosmosNetwork)>,
    pub(crate) client: reqwest::Client,
    pub(crate) fontdb: Database,
}

impl App {
    pub(crate) async fn new(opt: Opt) -> Result<App> {
        let postgres_uri = opt.pgopt.uri();
        let db = Db::new(&postgres_uri).await?;
        let mut cosmos_map = HashMap::new();
        for chain_id in ChainId::all() {
            let network = chain_id.into_cosmos_network()?;
            let mut builder = network.builder().await?;

            let grpc = match chain_id {
                ChainId::Atlantic2
                | ChainId::Dragonfire4
                | ChainId::Elgafar1
                | ChainId::Juno1
                | ChainId::OsmoTest5
                | ChainId::Stargaze1
                | ChainId::Uni6
                | ChainId::Injective888 => None,
                ChainId::Osmosis1 => {
                    Some((&opt.osmosis_mainnet_primary, &opt.osmosis_mainnet_fallbacks))
                }
                ChainId::Pacific1 => Some((&opt.sei_mainnet_primary, &opt.sei_mainnet_fallbacks)),
                ChainId::Injective1 => Some((
                    &opt.injective_mainnet_primary,
                    &opt.injective_mainnet_fallbacks,
                )),
            };

            if let Some((primary, fallbacks)) = grpc {
                builder.set_grpc_url(primary);
                for fallback in fallbacks {
                    builder.add_grpc_fallback_url(fallback);
                }
            }

            builder.set_referer_header(Some("https://indexer.levana.exchange/".to_owned()));

            let cosmos = builder.build_lazy()?;
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

        // Load up the fonts and convert text values
        let mut fontdb = resvg::usvg::fontdb::Database::new();
        fontdb.load_system_fonts();

        Ok(App {
            cosmos: cosmos_map,
            opt,
            db,
            factories,
            client,
            fontdb,
        })
    }

    pub(crate) async fn migrate_db(&self) -> Result<()> {
        sqlx::migrate!("src/bin/perps-companion/migrations")
            .run(&self.db.pool)
            .await
            .context("Error while running database migrations")
    }
}
