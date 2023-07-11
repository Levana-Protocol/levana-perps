use anyhow::{Context, Result};
use cosmos::{CosmosNetwork, TxBuilder};
use perps_exes::{
    config::{ChainConfig, PythConfig},
    pyth::{get_oracle_update_msg, VecWithCurr},
};
use shared::storage::MarketId;

#[derive(clap::Parser)]
pub(crate) struct UtilOpt {
    #[clap(subcommand)]
    sub: Sub,
}

#[derive(clap::Parser)]
enum Sub {
    /// Set the price in a Pyth oracle
    UpdatePyth {
        #[clap(flatten)]
        inner: UpdatePythOpt,
    },
}

impl UtilOpt {
    pub(crate) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        match self.sub {
            Sub::UpdatePyth { inner } => update_pyth_opt(opt, inner).await,
        }
    }
}

#[derive(clap::Parser)]
struct UpdatePythOpt {
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: CosmosNetwork,
    /// Market ID to do the update for
    #[clap(long)]
    market: MarketId,
}

async fn update_pyth_opt(
    opt: crate::cli::Opt,
    UpdatePythOpt { market, network }: UpdatePythOpt,
) -> Result<()> {
    let basic = opt.load_basic_app(network).await?;
    let pyth = PythConfig::load()?;
    let endpoints = VecWithCurr::new(pyth.endpoints.clone());
    let client = reqwest::Client::new();
    let feeds = pyth
        .markets
        .get(&market)
        .with_context(|| format!("No Pyth feed data found for {market}"))?;

    let chain = ChainConfig::load(network)?;
    let oracle = basic.cosmos.make_contract(
        chain
            .pyth
            .with_context(|| format!("No Pyth oracle found for network {network}"))?,
    );

    let msg = get_oracle_update_msg(&feeds, &basic.wallet, &endpoints, &client, &oracle).await?;

    let builder = TxBuilder::default().add_message(msg);
    let res = builder
        .sign_and_broadcast(&basic.cosmos, &basic.wallet)
        .await?;
    log::info!("Price set in: {}", res.txhash);
    Ok(())
}
