use std::collections::HashMap;

use anyhow::Result;
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
        let db = Db::new(&opt.postgres_uri).await?;
        Ok(App {
            cosmos: ChainId::all()
                .into_iter()
                .map(|chain_id| {
                    (
                        chain_id,
                        chain_id.into_cosmos_network().builder().build_lazy(),
                    )
                })
                .collect(),
            opt,
            db,
        })
    }
}
