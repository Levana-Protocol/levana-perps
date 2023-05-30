use cosmos::CosmosNetwork;
use perps_exes::prelude::*;

use crate::{cli::Opt, init_chain::TRACKER};

#[derive(clap::Parser)]
pub(crate) struct MainnetOpt {
    #[clap(subcommand)]
    sub: Sub,
}

#[derive(clap::Parser)]
enum Sub {
    /// Store the tracker code on chain
    StoreTracker {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: CosmosNetwork,
    },
    /// Instantiate a new tracker
    InstantiateTracker {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: CosmosNetwork,
        /// Tracker code ID
        #[clap(long)]
        tracker_code_id: u64,
    },
    /// Migrate an existing tracker to a newer code version
    MigrateTracker {
        /// Network to use.
        #[clap(long, env = "COSMOS_NETWORK")]
        network: CosmosNetwork,
        /// Tracker code ID
        #[clap(long)]
        tracker_code_id: u64,
    },
}

pub(crate) async fn go(opt: Opt, inner: MainnetOpt) -> Result<()> {
    match inner.sub {
        Sub::StoreTracker { network } => store_tracker(opt, network).await?,
        Sub::InstantiateTracker {
            network,
            tracker_code_id,
        } => instantiate_tracker(opt, network, tracker_code_id).await?,
        Sub::MigrateTracker {
            network,
            tracker_code_id,
        } => migrate_tracker(opt, network, tracker_code_id).await?,
    }
    Ok(())
}

async fn store_tracker(opt: Opt, network: CosmosNetwork) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;

    log::info!("Storing tracker code...");
    let tracker_code_id = app
        .cosmos
        .store_code_path(&app.wallet, opt.get_contract_path(TRACKER))
        .await?;

    log::info!("New tracker code ID for network {network} is {tracker_code_id}");

    Ok(())
}

async fn instantiate_tracker(opt: Opt, network: CosmosNetwork, tracker_code_id: u64) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;
    let contract = app
        .cosmos
        .make_code_id(tracker_code_id)
        .instantiate(
            &app.wallet,
            "Levana Contract Tracker",
            vec![],
            msg::contracts::tracker::entry::InstantiateMsg {},
        )
        .await?;
    log::info!("New tracker contract is: {contract}");
    log::info!("Please store in the config-chain.yaml file");
    Ok(())
}

async fn migrate_tracker(opt: Opt, network: CosmosNetwork, tracker_code_id: u64) -> Result<()> {
    let app = opt.load_app_mainnet(Some(network), None).await?;
    let tracker = app
        .tracker
        .with_context(|| format!("No tracker found for network {network}"))?;
    tracker
        .0
        .migrate(
            &app.wallet,
            tracker_code_id,
            msg::contracts::tracker::entry::MigrateMsg {},
        )
        .await?;
    log::info!("Tracker contract {} is migrated", tracker.0);
    Ok(())
}
