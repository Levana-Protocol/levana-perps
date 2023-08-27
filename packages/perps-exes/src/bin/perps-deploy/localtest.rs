use cosmos::{CosmosNetwork, HasAddress, HasAddressType, RawWallet};
use msg::prelude::*;
use perps_exes::PerpApp;

use std::{
    process::{Child, Command, Stdio},
    time::Duration,
};

use crate::chain_tests::{
    test_funding_market, test_pnl_on_liquidation, test_set_and_fetch_price, test_update_leverage,
    test_update_max_gains,
};
use crate::{
    chain_tests::{test_update_collateral, test_wallet_balance_decrease},
    cli::Opt,
    instantiate::InstantiateResponse,
    local_deploy::{self, LocalDeployOpt},
};

#[derive(clap::Parser)]
pub(crate) struct TestsOpt {
    /// Network to use. Either this or family must be provided.
    #[clap(long, env = "COSMOS_NETWORK")]
    pub(crate) network: CosmosNetwork,
    /// Skip initialization
    #[clap(long)]
    skip_init: bool,
    /// Market we want to interact with
    #[clap(
        long,
        env = "LEVANA_PERP_MARKET_ID",
        global = true,
        default_value = "ATOM_USD"
    )]
    pub market_id: MarketId,
}

struct OsmoLocalProcess(Child);

fn kill_osmo_local() {
    match Command::new("docker")
        .arg("stop")
        .arg("osmolocaltest")
        .status()
    {
        Ok(ec) => {
            if ec.success() {
                log::info!("Successfully killed osmolocal");
            } else {
                log::info!("Killing osmolocal exited with {ec:?}");
            }
        }
        Err(e) => log::info!("Problem killing junolocal: {e:?}"),
    }
}

impl Drop for OsmoLocalProcess {
    fn drop(&mut self) {
        kill_osmo_local()
    }
}

impl OsmoLocalProcess {
    fn launch() -> Result<Self> {
        kill_osmo_local();
        log::info!("Going to spawn new osmolocal");
        Ok(OsmoLocalProcess(
            Command::new("./.ci/osmolocal.sh")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        ))
    }
}

fn init_process(skip_init: bool) -> Result<Option<OsmoLocalProcess>> {
    if skip_init {
        Ok(None)
    } else {
        Ok(Some(OsmoLocalProcess::launch()?))
    }
}

pub(crate) async fn go(opt: Opt, opts: TestsOpt) -> Result<()> {
    let mut ol = init_process(opts.skip_init)?;

    let raw_wallet = opt.wallet.clone().context("No wallet provided")?;
    let network = opts.network;

    if let Some(ol) = &mut ol {
        log::info!("Waiting till Network is up");
        wait_till_network_is_up(raw_wallet.clone(), network, ol).await?;
    }

    log::info!("Going to Deploy");

    let InstantiateResponse {
        factory,
        markets: _,
    } = local_deploy::go(
        opt.clone(),
        LocalDeployOpt {
            network,
            initial_price: "9.5".parse()?,
            collateral_price: "10".parse()?,
        },
    )
    .await?;

    let perp_app = PerpApp::new(raw_wallet, factory, None, opts.market_id, network).await?;

    test_funding_market(&perp_app).await?;
    test_wallet_balance_decrease(&perp_app).await?;
    test_update_collateral(&perp_app).await?;
    test_set_and_fetch_price(&perp_app).await?;
    test_update_leverage(&perp_app).await?;
    test_update_max_gains(&perp_app).await?;
    test_pnl_on_liquidation(&perp_app).await?;

    Ok(())
}

async fn wait_till_network_is_up(
    wallet: RawWallet,
    network: CosmosNetwork,
    ol: &mut OsmoLocalProcess,
) -> Result<()> {
    let total_estimated_seconds = Duration::from_secs(15);
    let retry_seconds = Duration::from_millis(100);
    let total_counter = total_estimated_seconds.as_millis() / retry_seconds.as_millis();

    for counter in 1..=total_counter {
        if counter % 10 == 0 {
            log::info!("Trying to connect to the network ({counter}/{total_counter})");
        }

        if let Some(exit_status) = ol.0.try_wait()? {
            anyhow::bail!("localosmo child process exited early with exit status: {exit_status}");
        }

        let builder = network.builder().await?;
        let cosmos = builder.build().await;
        let cosmos = match cosmos {
            Ok(cosmos) => cosmos,
            Err(_) => {
                tokio::time::sleep(retry_seconds).await;
                continue;
            }
        };
        let address_type = cosmos.get_address_type();
        let wallet = wallet.for_chain(address_type)?;

        let balances = cosmos.all_balances(wallet.get_address_string()).await;
        if balances.is_ok() {
            return Ok(());
        } else {
            tokio::time::sleep(retry_seconds).await;
        }
    }
    Err(anyhow!("Unable to connect to the network"))
}
