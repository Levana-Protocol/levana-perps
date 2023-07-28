use std::collections::HashMap;

use anyhow::{Context, Result};
use cosmos::Cosmos;

use crate::{cli::Opt, db::handle::Db, types::ChainId};

pub(crate) struct App {
    /// Map from chain ID to a Cosmos connection
    #[allow(dead_code)]
    pub(crate) cosmos: HashMap<ChainId, Cosmos>,
    pub(crate) opt: Opt,
    pub(crate) db: Db,
}

impl App {
    pub(crate) async fn new(opt: Opt) -> Result<App> {
        let postgres_uri = opt
            .postgres_uri
            .clone()
            .or_else(|| {
                opt.pgopt.as_ref().map(|pgopt| {
                    format!(
                        "postgresql://{}:{}@{}:{}/{}",
                        pgopt.user, pgopt.password, pgopt.host, pgopt.port, pgopt.database
                    )
                })
            }) // no password escaping considered
            .unwrap();
        let db = Db::new(&postgres_uri).await?;
        let mut cosmos_map = HashMap::new();
        for chain_id in ChainId::all() {
            let cosmos = chain_id.into_cosmos_network().builder().await?.build_lazy();
            cosmos_map.insert(chain_id, cosmos);
        }
        Ok(App {
            cosmos: cosmos_map,
            opt,
            db,
        })
    }

    pub(crate) async fn migrate_db(&self) -> Result<()> {
        sqlx::migrate!("src/bin/perps-companion/migrations")
            .run(&self.db.pool)
            .await
            .context("Error while running database migrations")
    }
}
