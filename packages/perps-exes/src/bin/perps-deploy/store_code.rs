use anyhow::Result;
use cosmos::{Cosmos, CosmosNetwork, Wallet};
use msg::contracts::tracker::entry::CodeIdResp;

use crate::{app::get_suffix_network, cli::Opt, tracker::Tracker, util::get_hash_for_path};

#[derive(clap::Parser)]
pub(crate) struct StoreCodeOpt {
    /// Family name for these contracts. Either this or network must be provided.
    #[clap(long, env = "PERPS_FAMILY", global = true)]
    family: Option<String>,
    /// Network to use. Either this or family must be provided.
    #[clap(long, env = "COSMOS_NETWORK", global = true)]
    network: Option<CosmosNetwork>,
}

pub(crate) const CW20: &str = "cw20";
pub(crate) const FACTORY: &str = "factory";
pub(crate) const LIQUIDITY_TOKEN: &str = "liquidity_token";
pub(crate) const MARKET: &str = "market";
pub(crate) const POSITION_TOKEN: &str = "position_token";

const PROTOCOL_CONTRACT_TYPES: [&str; 4] = [FACTORY, LIQUIDITY_TOKEN, MARKET, POSITION_TOKEN];

pub(crate) async fn go(opt: Opt, StoreCodeOpt { family, network }: StoreCodeOpt) -> Result<()> {
    let network = match (family, network) {
        (None, None) => anyhow::bail!("Please specify either family or network"),
        (None, Some(network)) => network,
        (Some(family), _) => {
            let from_family = get_suffix_network(&family)?.1;
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
    let (tracker, _) = basic.get_tracker_faucet()?;

    store_code(
        &opt,
        &basic.cosmos,
        &basic.wallet,
        &tracker,
        &PROTOCOL_CONTRACT_TYPES,
    )
    .await
}

pub(crate) async fn store_code(
    opt: &Opt,
    cosmos: &Cosmos,
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
                let code_id = cosmos.store_code_path(wallet, &path).await?.get_code_id();
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
