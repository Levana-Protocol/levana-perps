use std::{fs::File, path::PathBuf};

use cosmos::{Address, HasAddress};
use csv::Writer;
use perps_exes::{
    config::{MainnetFactories, MainnetFactory},
    contracts::Factory,
};
use perpswap::prelude::*;

#[derive(clap::Parser)]
pub(super) struct ListContractsOpt {
    /// Factory contracts. Can be the label or the address.
    #[clap(long)]
    factory: Vec<String>,
    /// Destination CSV file.
    #[clap(long)]
    output: PathBuf,
}

impl ListContractsOpt {
    pub(super) async fn go(self) -> Result<()> {
        let ListContractsOpt { factory, output } = self;
        let mut csv = csv::Writer::from_path(&output)?;
        let factories = MainnetFactories::load()?;
        for factory in factory {
            go(factories.get(&factory)?, &mut csv).await?;
        }
        Ok(())
    }
}

async fn go(factory: &MainnetFactory, csv: &mut Writer<File>) -> Result<()> {
    let builder = factory.network.builder().await?;
    let chain_id = builder.chain_id().to_owned();
    let cosmos = builder.build()?;
    let factory = Factory::from_contract(cosmos.make_contract(factory.address));
    for market in factory.get_markets().await? {
        csv.serialize(Record {
            chain: &chain_id,
            market_id: &market.market_id,
            market_contract: market.market.get_address(),
        })?;
        csv.flush()?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct Record<'a> {
    chain: &'a str,
    market_id: &'a MarketId,
    market_contract: Address,
}
