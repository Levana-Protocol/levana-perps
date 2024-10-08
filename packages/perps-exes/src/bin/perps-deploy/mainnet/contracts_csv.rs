use std::path::PathBuf;

use anyhow::Result;
use cosmos::{Address, HasAddress};
use perps_exes::{
    config::MainnetFactories,
    contracts::{Factory, MarketInfo},
};
use perpswap::storage::MarketId;

#[derive(clap::Parser)]
pub(super) struct ContractsCsvOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// Destination file
    #[clap(long)]
    csv: PathBuf,
}
impl ContractsCsvOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    ContractsCsvOpts { factory, csv }: ContractsCsvOpts,
) -> Result<()> {
    let mut csv = ::csv::Writer::from_path(&csv)?;
    #[derive(serde::Serialize)]
    struct Record<'a> {
        kind: &'a str,
        market: Option<&'a MarketId>,
        address: Address,
    }

    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;

    csv.serialize(Record {
        kind: "factory",
        market: None,
        address: factory.address,
    })?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));
    let markets = factory.get_markets().await?;

    for MarketInfo {
        market_id,
        market,
        position_token,
        liquidity_token_lp,
        liquidity_token_xlp,
    } in markets
    {
        csv.serialize(Record {
            kind: "market",
            market: Some(&market_id),
            address: market.get_address(),
        })?;
        csv.serialize(Record {
            kind: "position-token",
            market: Some(&market_id),
            address: position_token.get_address(),
        })?;
        csv.serialize(Record {
            kind: "liquidity-token-lp",
            market: Some(&market_id),
            address: liquidity_token_lp.get_address(),
        })?;
        csv.serialize(Record {
            kind: "liquidity-token-xlp",
            market: Some(&market_id),
            address: liquidity_token_xlp.get_address(),
        })?;
    }

    Ok(())
}
