use anyhow::Result;
use cosmos::{CosmosNetwork, Address};
use msg::contracts::hatching::entry::MigrateMsg as HatchMigrateMsg;

use crate::{
    cli::Opt,
    store_code::{Contracts, HATCHING},
};

#[derive(clap::Parser)]
pub(crate) struct MigrateRewardsOpt {
    /// Contracts to migrate 
    #[clap(long, env = "CONTRACTS")]
    pub(crate) contracts: Contracts,
    /// Network to use
    #[clap(long, env = "COSMOS_NETWORK")]
    pub(crate) network: CosmosNetwork,

    /// hatching contract address
    #[clap(
        long,
        env = "HATCH_ADDRESS",
        default_value = "juno15nmqu8s7ywcacm3755eg7024vfqchxm3tytqgzdv94uwm6a62n6qc8r0uz"
    )]
    pub hatch_address: Address,
}

pub(crate) async fn go(global_opt: Opt, opt: MigrateRewardsOpt) -> Result<()> {
    let basic = global_opt.load_basic_app(opt.network).await?;
    let (tracker, _) = basic.get_tracker_and_faucet()?;


    match opt.contracts {
        Contracts::Hatching => {
            let code_id = tracker.require_code_by_type(&global_opt, HATCHING).await?.get_code_id();
            let contract = basic.cosmos.make_contract(opt.hatch_address);
            let msg = HatchMigrateMsg {};
            contract.migrate(&basic.wallet, code_id, msg).await?;

            println!("migrated hatching contract, code id: {code_id}, address: {}", opt.hatch_address);
        },
        _ => {
            anyhow::bail!("TODO: only hatching contracts can be migrated right now")
        }
    }


    Ok(())
}