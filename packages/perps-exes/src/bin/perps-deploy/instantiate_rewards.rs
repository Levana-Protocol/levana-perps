use std::{collections::HashSet, str::FromStr};

use crate::{
    app::BasicApp,
    cli::Opt,
    store_code::{Contracts, HATCHING, IBC_EXECUTE_PROXY, LVN_REWARDS},
};
use anyhow::{bail, Context, Result};
use cosmos::Coin;
use cosmos::{Contract, CosmosNetwork, HasAddress};
use cosmwasm_std::IbcOrder;
use msg::contracts::hatching::ibc::IbcChannelVersion;
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
    } = inst_opt;

    let basic = opt.load_basic_app(network).await?;
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
                    let mint_contract =
                        instantiate_testnet_nft_contract(&basic, "Levana Baby Dragons Mock")
                            .await?;

                    let ibc_contract = tracker
                        .require_code_by_type(&opt, IBC_EXECUTE_PROXY)
                        .await?
                        .instantiate(
                            &basic.wallet,
                            format!("Levana IbcExecute{label_suffix}"),
                            vec![],
                            msg::contracts::ibc_execute_proxy::entry::InstantiateMsg {
                                contract: mint_contract.get_address_string().into(),
                                ibc_channel_version: IbcChannelVersion::NftMint
                                    .as_str()
                                    .to_string(),
                                ibc_channel_order: IbcOrder::Unordered,
                            },
                        )
                        .await?;

                    let mut minters = HashSet::new();
                    minters.insert(ibc_contract.get_address_string());
                    mint_contract
                        .execute(&basic.wallet, vec![], NftExecuteMsg::AddMinters { minters })
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
                    &basic.wallet,
                    format!("Levana Hatching{label_suffix}"),
                    vec![],
                    msg::contracts::hatching::entry::InstantiateMsg {
                        burn_egg_contract: burn_egg_contract.clone().into(),
                        burn_dust_contract: burn_dust_contract.clone().into(),
                        profile_contract: profile_contract.clone().into(),
                    },
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
                        &basic.wallet,
                        vec![],
                        NftExecuteMsg::AddMinters {
                            minters: minters.clone(),
                        },
                    )
                    .await?;
                basic
                    .cosmos
                    .make_contract(burn_dust_contract.parse()?)
                    .execute(&basic.wallet, vec![], NftExecuteMsg::AddMinters { minters })
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
                        &basic.wallet,
                        vec![],
                        ProfileExecuteMsg::Admin {
                            message: ProfileAdminExecuteMsg::AddAdmin {
                                addr: contract.get_address_string(),
                            },
                        },
                    )
                    .await?;
            }

            let info = contract.info().await?;

            log::info!("new hatching deployed at {}", contract.get_address_string());
            log::info!("hatching ibc port is {}", info.ibc_port_id);
        }
        Contracts::LvnRewards => {
            let code_id = tracker.require_code_by_type(&opt, LVN_REWARDS).await?;
            let contract = code_id
                .instantiate(
                    &basic.wallet,
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
                )
                .await?;

            let info = contract.info().await?;

            log::info!(
                "new lvn rewards deployed at {}",
                contract.get_address_string()
            );
            log::info!("lvn rewards ibc port is {}", info.ibc_port_id);

            if network != CosmosNetwork::OsmosisMainnet {
                let amount = "1000".to_string();
                log::info!(
                    "giving {amount} of {lvn_denom} to {}",
                    contract.get_address()
                );
                let coin = Coin {
                    denom: lvn_denom,
                    amount: "10000".to_string(),
                };

                // gas is wildly underestimated on osmosis testnet at least
                // bump up the multiplier to make sure we have enough
                // then set it back when we're done sending coins
                let original_gas_multiplier = basic.cosmos.get_gas_multiplier();
                basic.cosmos.set_gas_multiplier(1.5);

                basic
                    .wallet
                    .send_coins(&basic.cosmos, contract.get_address(), vec![coin.clone()])
                    .await?;

                basic.cosmos.set_gas_multiplier(original_gas_multiplier);
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

    let contract = app
        .cosmos
        .make_code_id(code_id)
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

    let contract = app
        .cosmos
        .make_code_id(code_id)
        .instantiate(
            &app.wallet,
            label.clone(),
            vec![],
            InstantiateMsg {
                dragons: eggs_contract.into(),
                loot: dust_contract.into(),
                dragon_rider: dragon_riders_contract.into(),
                admin: app.wallet.get_address_string(),
                withdraw_dest: app.wallet.get_address_string(),
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NftExecuteMsg {
    AddMinters { minters: HashSet<String> },
}
