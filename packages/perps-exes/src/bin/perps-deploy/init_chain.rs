use anyhow::Result;
use cosmos::{ContractAdmin, CosmosNetwork};
use msg::contracts::faucet::entry::GasAllowance;

use crate::cli::Opt;
use crate::store_code::CW20;

#[derive(clap::Parser)]
pub(crate) struct InitChainOpt {
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: CosmosNetwork,
    /// Number of seconds to limit taps to
    #[clap(long, default_value = "300")]
    tap_limit: u32,
    /// Gas to send to faucet on initialization, given in coins (e.g. 1 == 1000000uatom)
    #[clap(long, default_value = "20000")]
    gas_to_send: u128,
    /// Amount of gas (in microunits) to send with a faucet tap
    #[clap(long, default_value = "1000000")]
    gas_allowance: u128,
}

const FAUCET: &str = "faucet";
const TRACKER: &str = "tracker";

pub(crate) async fn go(
    opt: Opt,
    InitChainOpt {
        network,
        tap_limit,
        gas_to_send,
        gas_allowance,
    }: InitChainOpt,
) -> Result<()> {
    let app = opt.load_basic_app(network).await?;
    let gas_coin = app.cosmos.get_cosmos_builder().gas_coin().to_owned();

    log::info!("Storing code...");
    let wallet = app.get_wallet()?;
    let cw20_code_id = app
        .cosmos
        .store_code_path(wallet, opt.get_contract_path(CW20))
        .await?;
    log::info!("CW20: {cw20_code_id}");
    let faucet_code_id = app
        .cosmos
        .store_code_path(wallet, opt.get_contract_path(FAUCET))
        .await?;
    log::info!("Faucet: {faucet_code_id}");
    let tracker_code_id = app
        .cosmos
        .store_code_path(wallet, opt.get_contract_path(TRACKER))
        .await?;
    log::info!("Tracker: {tracker_code_id}");

    log::info!("Instantiating tracker");

    let tracker = tracker_code_id
        .instantiate(
            wallet,
            "Levana Perps Tracker",
            vec![],
            msg::contracts::tracker::entry::InstantiateMsg {},
            ContractAdmin::Sender,
        )
        .await?;
    log::info!("New tracker contract: {tracker}");

    let faucet = faucet_code_id
        .instantiate(
            wallet,
            "Levana Perps Faucet",
            vec![],
            msg::contracts::faucet::entry::InstantiateMsg {
                tap_limit: Some(tap_limit),
                cw20_code_id: cw20_code_id.get_code_id(),
                gas_allowance: Some(GasAllowance {
                    denom: gas_coin,
                    amount: gas_allowance.into(),
                }),
            },
            ContractAdmin::Sender,
        )
        .await?;
    log::info!("New faucet contract: {faucet}");

    log::info!("Sending gas funds to faucet");
    let res = wallet
        .send_gas_coin(&app.cosmos, &faucet, gas_to_send * 1_000_000)
        .await?;
    log::info!("Gas sent in {}", res.txhash);

    log::info!("Please remember to update assets/config.yaml with the new addresses!");

    // In the future, do we want to automatically add admins?

    Ok(())
}
