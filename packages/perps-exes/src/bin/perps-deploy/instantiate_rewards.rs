use std::{collections::HashSet, path::PathBuf, str::FromStr};

use crate::{
    app::BasicApp,
    cli::Opt,
    store_code::{Contracts, HATCHING, IBC_EXECUTE_PROXY, LVN_REWARDS},
};
use anyhow::{bail, Context, Result};
use cosmos::{Coin, ContractAdmin, CosmosBuilder, TokenFactory};
use cosmos::{Contract, CosmosNetwork, HasAddress};
use cosmwasm_std::IbcOrder;
use msg::contracts::hatching::{
    dragon_mint::DragonMintExtra, entry::ExecuteMsg as HatchingExecuteMsg, ibc::IbcChannelVersion,
};
use serde::{Deserialize, Serialize};

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
    /// If deploying ibc_execute_proxy, specify the target contract it's proxying
    #[clap(long)]
    pub(crate) ibc_execute_proxy_target: Option<IbcExecuteProxyTarget>,
    /// If deploying ibc_execute_proxy, specify the target contract it's proxying
    #[clap(
        long,
        default_value = "factory/osmo12g96ahplpf78558cv5pyunus2m66guykt96lvc/lvn1"
    )]
    pub(crate) lvn_denom: String,
    /// Path to hatchery so we can load the CSV for babydragon extra meta
    #[clap(long, default_value = "../levana-hatchery")]
    pub(crate) path_to_hatchery: PathBuf,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum IbcExecuteProxyTarget {
    NftMint,
}
impl FromStr for IbcExecuteProxyTarget {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "nft-mint" => Ok(IbcExecuteProxyTarget::NftMint),
            _ => Err(anyhow::anyhow!("Unknown ibc execute proxy: {s}")),
        }
    }
}

pub(crate) async fn go(opt: Opt, inst_opt: InstantiateRewardsOpt) -> Result<()> {
    let InstantiateRewardsOpt {
        network,
        contracts,
        prod,
        ibc_execute_proxy_target: ibc_execute_proxy,
        lvn_denom,
        path_to_hatchery,
    } = inst_opt;

    let basic = opt.load_basic_app(network).await?;
    let wallet = basic.get_wallet()?;
    let (tracker, _) = basic.get_tracker_and_faucet()?;

    let label_suffix = if prod { "" } else { " (testnet)" };

    match contracts {
        Contracts::PerpsProtocol => {
            bail!("Cannot instantiate perps-protocol with instantiate-rewards, use regular instantiate instead");
        }
        Contracts::IbcExecuteProxy => {
            match ibc_execute_proxy
                .context("Must specify --ibc-execute-proxy when instantiating ibc-execute")?
            {
                IbcExecuteProxyTarget::NftMint => {
                    let mint_contract = if network == CosmosNetwork::JunoMainnet {
                        bail!("no mint contract on mainnet!")
                    } else {
                        instantiate_testnet_nft_contract(&basic, "Levana Baby Dragons Mock").await?
                    };

                    let ibc_contract = tracker
                        .require_code_by_type(&opt, IBC_EXECUTE_PROXY)
                        .await?
                        .instantiate(
                            wallet,
                            format!("Levana IbcExecute{label_suffix}"),
                            vec![],
                            msg::contracts::ibc_execute_proxy::entry::InstantiateMsg {
                                contract: mint_contract.get_address_string().into(),
                                ibc_channel_version: IbcChannelVersion::NftMint
                                    .as_str()
                                    .to_string(),
                                ibc_channel_order: IbcOrder::Unordered,
                            },
                            ContractAdmin::Sender,
                        )
                        .await?;

                    let mut minters = HashSet::new();
                    minters.insert(ibc_contract.get_address_string());
                    mint_contract
                        .execute(wallet, vec![], NftExecuteMsg::AddMinters { minters })
                        .await?;

                    let info = ibc_contract.info().await?;

                    log::info!(
                        "ibc-execute for minting contract deployed at {}",
                        ibc_contract.get_address_string()
                    );
                    log::info!("ibc-execute for minting ibc port is {}", info.ibc_port_id);
                }
            };
        }
        Contracts::Hatching => {
            let burn_egg_contract = if network == CosmosNetwork::JunoMainnet {
                // eggs are in dragon contract
                "juno1a90f8jdwm4h43yzqgj4xqzcfxt4l98ev970vwz6l9m02wxlpqd2squuv6k".to_string()
            } else {
                instantiate_testnet_nft_contract(&basic, "Levana Egg/Dragons Mock")
                    .await?
                    .get_address_string()
            };

            let burn_dust_contract = if network == CosmosNetwork::JunoMainnet {
                // dust is in loot contract
                "juno1gmnkf4fs0qrwxdjcwngq3n2gpxm7t24g8n4hufhyx58873he85ss8q9va4".to_string()
            } else {
                instantiate_testnet_nft_contract(&basic, "Levana Dust/Loot Mock")
                    .await?
                    .get_address_string()
            };

            let profile_contract = if network == CosmosNetwork::JunoMainnet {
                "juno12fdnmycnuvhua3y9pzxweu2eqqv77k454h0w8vwjjajvjrawuaksfn88u9".to_string()
            } else {
                let dragon_riders_contract =
                    instantiate_testnet_nft_contract(&basic, "Levana Dragon Riders Mock")
                        .await?
                        .get_address_string();

                instantiate_testnet_profile_contract(
                    &basic,
                    "Levana Profile Mock",
                    &burn_egg_contract,
                    &burn_dust_contract,
                    &dragon_riders_contract,
                )
                .await?
                .get_address_string()
            };

            let code_id = tracker.require_code_by_type(&opt, HATCHING).await?;
            let contract = code_id
                .instantiate(
                    wallet,
                    format!("Levana Hatching{label_suffix}"),
                    vec![],
                    msg::contracts::hatching::entry::InstantiateMsg {
                        burn_egg_contract: burn_egg_contract.clone().into(),
                        burn_dust_contract: burn_dust_contract.clone().into(),
                        profile_contract: profile_contract.clone().into(),
                    },
                    ContractAdmin::Sender,
                )
                .await?;

            if network != CosmosNetwork::JunoMainnet {
                log::info!("giving hatching contract nft burn permissions");
                let mut minters = HashSet::new();
                minters.insert(contract.get_address_string());
                basic
                    .cosmos
                    .make_contract(burn_egg_contract.parse()?)
                    .execute(
                        wallet,
                        vec![],
                        NftExecuteMsg::AddMinters {
                            minters: minters.clone(),
                        },
                    )
                    .await?;
                basic
                    .cosmos
                    .make_contract(burn_dust_contract.parse()?)
                    .execute(wallet, vec![], NftExecuteMsg::AddMinters { minters })
                    .await?;

                log::info!("giving hatching contract profile admin permissions");

                #[derive(Serialize, Deserialize)]
                #[serde(rename_all = "snake_case")]
                enum ProfileExecuteMsg {
                    Admin { message: ProfileAdminExecuteMsg },
                }
                #[derive(Serialize, Deserialize)]
                #[serde(rename_all = "snake_case")]
                pub enum ProfileAdminExecuteMsg {
                    AddAdmin { addr: String },
                }
                basic
                    .cosmos
                    .make_contract(profile_contract.parse()?)
                    .execute(
                        wallet,
                        vec![],
                        ProfileExecuteMsg::Admin {
                            message: ProfileAdminExecuteMsg::AddAdmin {
                                addr: contract.get_address_string(),
                            },
                        },
                    )
                    .await?;
            }

            log::info!("uploading babydragon mint info...");
            let filepath = path_to_hatchery
                .join("data")
                .join("juno-warp")
                .join("baby-dragons-extra.csv");
            let mut rdr = csv::Reader::from_path(filepath)?;
            let dragons: Vec<DragonMintExtra> = rdr
                .deserialize::<DragonMintExtra>()
                .map(|x| x.map_err(|e| e.into()))
                .collect::<Result<Vec<_>>>()?;
            const CHUNK_SIZE: usize = 1024;
            for (idx, chunk) in dragons.chunks(CHUNK_SIZE).enumerate() {
                println!(
                    "uploading {} to {} of {}",
                    idx * CHUNK_SIZE,
                    (idx * CHUNK_SIZE) + chunk.len(),
                    dragons.len()
                );
                let resp = contract
                    .execute(
                        wallet,
                        vec![],
                        HatchingExecuteMsg::SetBabyDragonExtras {
                            extras: chunk.to_vec(),
                        },
                    )
                    .await?;
                println!("tx hash: {:?}", resp.txhash);
            }

            let info = contract.info().await?;

            log::info!("new hatching deployed at {}", contract.get_address_string());
            log::info!("hatching ibc port is {}", info.ibc_port_id);
        }

        Contracts::LvnRewards => {
            let code_id = tracker.require_code_by_type(&opt, LVN_REWARDS).await?;
            let contract = code_id
                .instantiate(
                    wallet,
                    format!("Levana Rewards{label_suffix}"),
                    vec![],
                    msg::contracts::rewards::entry::InstantiateMsg {
                        config: msg::contracts::rewards::entry::ConfigUpdate {
                            token_denom: lvn_denom.clone(),
                            immediately_transferable: "0.25".parse()?,
                            unlock_duration_seconds: 10,
                            factory_addr:
                                "osmo17pxfdfeqwvrktzr7m76jdgksw2gsfqc95dqx6z6qqegcpuuv0xlqkpzej5"
                                    .to_string(),
                        },
                    },
                    ContractAdmin::Sender,
                )
                .await?;

            let info = contract.info().await?;

            log::info!(
                "new lvn rewards deployed at {}",
                contract.get_address_string()
            );
            log::info!("lvn rewards ibc port is {}", info.ibc_port_id);

            if network != CosmosNetwork::OsmosisMainnet {
                const AMOUNT: u128 = 100000000;
                log::info!(
                    "giving {AMOUNT} of {lvn_denom} to {}",
                    contract.get_address()
                );

                // gas is wildly underestimated on osmosis testnet at least
                // create a new cosmos instance with increased estimation multiplier
                // to make sure we have enough
                let mut builder = CosmosBuilder::clone(&*basic.cosmos.get_first_builder());
                builder.config.gas_estimate_multiplier = 1.5;
                let cosmos = builder.build_lazy().await;

                let tokenfactory = TokenFactory::new(basic.cosmos.clone(), wallet.clone());

                tokenfactory.mint(lvn_denom.clone(), AMOUNT).await?;

                let coin = Coin {
                    denom: lvn_denom,
                    amount: AMOUNT.to_string(),
                };

                wallet
                    .send_coins(&cosmos, contract.get_address(), vec![coin])
                    .await?;
            }
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
    let code_id: u64 = match app.network {
        CosmosNetwork::JunoTestnet => 1668,
        CosmosNetwork::StargazeTestnet => 2075,
        _ => bail!("nft contract is only supported on stargaze and juno testnets for now"),
    };

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

    let wallet = app.get_wallet()?;

    let contract = app
        .cosmos
        .make_code_id(code_id)
        .instantiate(
            wallet,
            label.clone(),
            vec![],
            InstantiateMsg {
                name: label.clone(),
                symbol: "WHTVR".to_string(),
                minter: vec![wallet.get_address_string()].into_iter().collect(),
                allow_burn: true,
                royalties: RoyaltyInfo::NoRoyalties {},
            },
            ContractAdmin::Sender,
        )
        .await?;

    log::info!(
        "instantiated {} at {}",
        label,
        contract.get_address_string()
    );

    Ok(contract)
}

async fn instantiate_testnet_profile_contract(
    app: &BasicApp,
    label: impl Into<String>,
    eggs_contract: impl Into<String>,
    dust_contract: impl Into<String>,
    dragon_riders_contract: impl Into<String>,
) -> Result<Contract> {
    let label: String = label.into();

    // was created by downloading the wasm from mainnet dragon contract
    // and uploading it to testnet
    let code_id: u64 = match app.network {
        CosmosNetwork::JunoTestnet => 1820,
        _ => bail!("profile contract is only supported on juno testnet for now"),
    };

    // just copy/pasted from levanamessages::nft
    #[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
    pub struct InstantiateMsg {
        /// Address of the Dragon NFT contract
        pub dragons: String,
        /// Address of the Loot NFT contract
        pub loot: String,
        /// Address of the Dragon Rider NFT contract
        pub dragon_rider: String,
        /// Primary admin of this contract
        pub admin: String,
        /// The wallet which receives funds during a withdrawal
        pub withdraw_dest: String,
    }

    let wallet = app.get_wallet()?;
    let contract = app
        .cosmos
        .make_code_id(code_id)
        .instantiate(
            wallet,
            label.clone(),
            vec![],
            InstantiateMsg {
                dragons: eggs_contract.into(),
                loot: dust_contract.into(),
                dragon_rider: dragon_riders_contract.into(),
                admin: wallet.get_address_string(),
                withdraw_dest: wallet.get_address_string(),
            },
            ContractAdmin::Sender,
        )
        .await?;

    log::info!(
        "instantiated {} at {}",
        label,
        contract.get_address_string()
    );

    Ok(contract)
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NftExecuteMsg {
    AddMinters { minters: HashSet<String> },
}
