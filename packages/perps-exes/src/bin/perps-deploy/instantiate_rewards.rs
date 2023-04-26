use std::collections::HashSet;

use crate::{
    app::BasicApp,
    cli::Opt,
    store_code::{Contracts, HATCHING},
};
use anyhow::{bail, Result};
use cosmos::{Contract, CosmosNetwork, HasAddress};

#[derive(clap::Parser)]
pub(crate) struct InstantiateRewardsOpt {
    /// Network to use
    #[clap(long, env = "COSMOS_NETWORK")]
    pub(crate) network: CosmosNetwork,
    /// Contracts to instantiate
    #[clap(long, env = "CONTRACTS")]
    pub(crate) contracts: Contracts,
    /// Is this a production deployment? Impacts labels used
    #[clap(long)]
    pub(crate) prod: bool,
}

pub(crate) async fn go(opt: Opt, inst_opt: InstantiateRewardsOpt) -> Result<()> {
    let InstantiateRewardsOpt {
        network,
        contracts,
        prod,
    } = inst_opt;

    let basic = opt.load_basic_app(network).await?;
    let (tracker, _) = basic.get_tracker_faucet()?;

    let label_suffix = if prod { "" } else { " (testnet)" };

    match contracts {
        Contracts::PerpsProtocol => {
            bail!("Cannot instantiate perps-protocol with instantiate-rewards, use regular instantiate instead");
        }
        Contracts::Hatching => {
            let code_id = tracker.require_code_by_type(&opt, HATCHING).await?;

            let burn_egg_contract = if network == CosmosNetwork::JunoMainnet {
                // eggs are in dragon contract
                "juno1a90f8jdwm4h43yzqgj4xqzcfxt4l98ev970vwz6l9m02wxlpqd2squuv6k".to_string()
            } else {
                instantiate_testnet_nft_contract(&basic, "Levana Dragons Mock")
                    .await?
                    .get_address_string()
            };

            let burn_dust_contract = if network == CosmosNetwork::JunoMainnet {
                // dust is in loot contract
                "juno1gmnkf4fs0qrwxdjcwngq3n2gpxm7t24g8n4hufhyx58873he85ss8q9va4".to_string()
            } else {
                instantiate_testnet_nft_contract(&basic, "Levana Loot Mock")
                    .await?
                    .get_address_string()
            };

            let contract = code_id
                .instantiate(
                    &basic.wallet,
                    format!("Levana Hatching{label_suffix}"),
                    vec![],
                    msg::contracts::hatching::entry::InstantiateMsg {
                        burn_egg_contract,
                        burn_dust_contract,
                    },
                )
                .await?;

            let info = contract.info().await?;

            log::info!("new hatching deployed at {}", contract.get_address_string());
            log::info!("hatching ibc port is {}", info.ibc_port_id);
        }
    }

    Ok(())
}

async fn instantiate_testnet_nft_contract(
    app: &BasicApp,
    label: impl Into<String>,
) -> Result<Contract> {
    let label: String = label.into();

    // was created by downloading the wasm from mainnet dragon contract
    // and uploading it to testnet
    const CODE_ID: u64 = 1668;

    // just copy/pasted from levanamessages::nft
    #[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
    pub struct InstantiateMsg {
        /// Name of the NFT contract
        pub name: String,
        /// Symbol of the NFT contract
        pub symbol: String,

        /// The minter is the only one who can create new NFTs.
        /// This is designed for a base NFT that is controlled by an external program
        /// or contract. You will likely replace this with custom logic in custom NFTs
        pub minter: HashSet<String>,
        /// Allow burning of the NFT. If true, allows burning.
        pub allow_burn: bool,
        pub royalties: RoyaltyInfo,
    }
    #[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
    #[serde(rename_all = "snake_case")]
    pub enum RoyaltyInfo {
        NoRoyalties {},
        Royalties {
            addr: String,
            /// Basis points, 100 == 1%
            basis_points: u32,
        },
    }

    let contract = app
        .cosmos
        .make_code_id(CODE_ID)
        .instantiate(
            &app.wallet,
            label.clone(),
            vec![],
            InstantiateMsg {
                name: label.clone(),
                symbol: "WHTVR".to_string(),
                minter: vec![app.wallet.get_address_string()].into_iter().collect(),
                allow_burn: true,
                royalties: RoyaltyInfo::NoRoyalties {},
            },
        )
        .await?;

    log::info!(
        "instantiated {} at {}",
        label,
        contract.get_address_string()
    );

    Ok(contract)
}
