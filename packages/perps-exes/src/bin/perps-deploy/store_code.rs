use std::{process::Output, str::FromStr};

use anyhow::Result;
use cosmos::{Cosmos, CosmosNetwork, Wallet};
use msg::contracts::tracker::entry::CodeIdResp;
use perps_exes::config::parse_deployment;

use crate::{cli::Opt, tracker::Tracker, util::get_hash_for_path};

#[derive(clap::Parser)]
pub(crate) struct StoreCodeOpt {
    /// Family name for these contracts. Either this or network must be provided.
    #[clap(long, env = "PERPS_FAMILY", global = true)]
    family: Option<String>,
    /// Network to use. Either this or family must be provided.
    #[clap(long, env = "COSMOS_NETWORK", global = true)]
    network: Option<CosmosNetwork>,

    /// Contract types to store. If not provided, the perps protocol suite of contracts will be stored.
    #[clap(
        long,
        env = "CONTRACTS",
        default_value = "perps-protocol",
        global = true
    )]
    contracts: Contracts,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Contracts {
    PerpsProtocol,
    Hatching,
    IbcExecuteProxy,
    LvnRewards,
}

impl FromStr for Contracts {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "perps-protocol" => Ok(Contracts::PerpsProtocol),
            "hatching" => Ok(Contracts::Hatching),
            "ibc-execute-proxy" => Ok(Contracts::IbcExecuteProxy),
            "lvn-rewards" => Ok(Contracts::LvnRewards),
            _ => Err(anyhow::anyhow!("Unknown contracts: {s}")),
        }
    }
}

impl Contracts {
    pub fn names(&self) -> &[&str] {
        match self {
            Contracts::PerpsProtocol => &[CW20, FACTORY, LIQUIDITY_TOKEN, MARKET, POSITION_TOKEN],
            Contracts::Hatching => &[HATCHING],
            Contracts::IbcExecuteProxy => &[IBC_EXECUTE_PROXY],
            Contracts::LvnRewards => &[LVN_REWARDS],
        }
    }
}

pub(crate) const CW20: &str = "cw20";
pub(crate) const FACTORY: &str = "factory";
pub(crate) const LIQUIDITY_TOKEN: &str = "liquidity_token";
pub(crate) const MARKET: &str = "market";
pub(crate) const POSITION_TOKEN: &str = "position_token";
pub(crate) const HATCHING: &str = "hatching";
pub(crate) const IBC_EXECUTE_PROXY: &str = "ibc_execute_proxy";
pub(crate) const LVN_REWARDS: &str = "rewards";

pub(crate) async fn go(
    opt: Opt,
    StoreCodeOpt {
        family,
        network,
        contracts,
    }: StoreCodeOpt,
) -> Result<()> {
    let network = match (family, network) {
        (None, None) => anyhow::bail!("Please specify either family or network"),
        (None, Some(network)) => network,
        (Some(family), _) => {
            let from_family = parse_deployment(&family)?.0;
            if let Some(network) = network {
                anyhow::ensure!(
                    network == from_family,
                    "Family and network parameters conflict, {from_family} vs {network}"
                );
            }
            from_family
        }
    };

    let basic = opt.load_basic_app(network).await?;
    let wallet = basic.get_wallet()?;
    let (tracker, _) = basic.get_tracker_and_faucet()?;

    store_code(
        &opt,
        &basic.cosmos,
        network,
        wallet,
        &tracker,
        contracts.names(),
    )
    .await
}

pub(crate) async fn store_code(
    opt: &Opt,
    cosmos: &Cosmos,
    network: CosmosNetwork,
    wallet: &Wallet,
    tracker: &Tracker,
    contract_types: &[&str],
) -> Result<()> {
    let gitrev = opt.get_gitrev()?;
    log::info!("Compiled WASM comes from gitrev {gitrev}");

    for ct in contract_types.iter().copied() {
        let path = opt.get_contract_path(ct);
        let hash = get_hash_for_path(&path)?;
        match tracker.get_code_by_hash(hash.clone()).await? {
            CodeIdResp::NotFound {} => {
                log::info!("Contract {ct} has SHA256 {hash} and is not on blockchain, uploading");
                let code_id = match (ct, network) {
                    ("market", CosmosNetwork::OsmosisTestnet) => {
                        store_market_cosmjs("osmosis").await?
                    }
                    ("market", CosmosNetwork::SeiTestnet) => store_market_cosmjs("sei").await?,
                    _ => cosmos.store_code_path(wallet, &path).await?.get_code_id(),
                };
                log::info!("Upload complete, new code ID is {code_id}, logging with the tracker");
                let res = tracker
                    .store_code(wallet, ct.to_owned(), code_id, hash, gitrev.clone())
                    .await?;
                log::info!(
                    "Contract stored, tracked in tracker with txhash {}",
                    res.txhash
                );
            }
            CodeIdResp::Found {
                contract_type,
                code_id,
                hash: hash2,
                tracked_at,
                gitrev,
            } => {
                anyhow::ensure!(contract_type == ct);
                anyhow::ensure!(hash == hash2);
                log::info!("Contract {ct} with SHA256 {hash} already uploaded with code ID {code_id} at {tracked_at} (from gitrev: {gitrev:?})");
            }
        }
    }

    Ok(())
}

async fn store_market_cosmjs(network: &str) -> Result<u64> {
    log::info!("Calling out to cosmjs script to upload market contract for {network}");
    let Output {
        status,
        stdout,
        stderr,
    } = tokio::process::Command::new("yarn")
        .current_dir("packages/perps-exes/cosmjs")
        .arg(format!("upload:{network}"))
        .output()
        .await?;
    if status.success() {
        let stdout = String::from_utf8(stdout).map_or_else(|x| format!("{x:?}"), |x| x);
        for line in stdout.lines() {
            if let Some(code_id) = line
                .strip_prefix("Contract uploaded with code ID ")
                .and_then(|x| u64::from_str(x).ok())
            {
                return Ok(code_id);
            }
        }
        let stderr = String::from_utf8(stderr).map_or_else(|x| format!("{x:?}"), |x| x);
        Err(anyhow::anyhow!(
            "Did not found code ID in output:\nstdout: {stdout}\nstderr: {stderr}"
        ))
    } else {
        let stdout = String::from_utf8(stdout).map_or_else(|x| format!("{x:?}"), |x| x);
        let stderr = String::from_utf8(stderr).map_or_else(|x| format!("{x:?}"), |x| x);
        Err(anyhow::anyhow!(
            "cosmjs failed.\nstdout: {stdout}\nstderr: {stderr}"
        ))
    }
}
