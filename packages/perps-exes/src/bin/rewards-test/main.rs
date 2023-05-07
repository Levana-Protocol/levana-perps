mod cli;
mod mock_nft;

use std::vec;

use crate::cli::{Cmd, Subcommand};
use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use cli::HatchEggOpt;
use cosmos::{Contract, Cosmos, HasAddressType, Wallet};
use mock_nft::Metadata;
use msg::contracts::hatching::{
    config::Config as HatchConfig,
    entry::{ExecuteMsg as HatchExecMsg, MaybeHatchStatusResp, QueryMsg as HatchQueryMsg},
};
use msg::contracts::rewards::entry::QueryMsg::RewardsInfo;
use msg::contracts::rewards::entry::RewardsInfoResp;
use perps_exes::prelude::*;

struct Hatch {
    #[allow(dead_code)]
    pub cosmos: Cosmos,
    pub wallet: Wallet,
    pub nft_admin_wallet: Wallet,
    pub contract: Contract,
    pub burn_egg_contract: Contract,
    pub config: HatchConfig,
}

impl Hatch {
    pub async fn new(opt: &HatchEggOpt) -> Result<Self> {
        let cosmos = opt.hatch_network.builder().build().await?;
        let address_type = cosmos.get_address_type();
        let wallet = opt.hatch_wallet.for_chain(address_type);
        let nft_admin_wallet = opt.mock_nft_admin_wallet.for_chain(address_type);

        let contract = Contract::new(cosmos.clone(), opt.hatch_address);

        let config: HatchConfig = contract.query(HatchQueryMsg::Config {}).await?;

        let burn_egg_contract = Contract::new(
            cosmos.clone(),
            config.nft_burn_contracts.egg.to_string().parse().unwrap(),
        );

        Ok(Self {
            cosmos,
            wallet,
            nft_admin_wallet,
            contract,
            burn_egg_contract,
            config,
        })
    }
}

struct NftMint {
    #[allow(dead_code)]
    pub cosmos: Cosmos,
    #[allow(dead_code)]
    pub wallet: Wallet,
    pub contract: Contract,
}

impl NftMint {
    pub async fn new(opt: &HatchEggOpt) -> Result<Self> {
        let cosmos = opt.nft_mint_network.builder().build().await?;
        let address_type = cosmos.get_address_type();
        let wallet = opt.nft_mint_wallet.for_chain(address_type);

        let contract = Contract::new(cosmos.clone(), opt.nft_mint_address);

        Ok(Self {
            cosmos,
            wallet,
            contract,
        })
    }
}

struct Rewards {
    pub cosmos: Cosmos,
    pub wallet: Wallet,
    pub contract: Contract,
}

impl Rewards {
    pub async fn new(opt: &HatchEggOpt) -> Result<Self> {
        let cosmos = opt.lvn_rewards_network.builder().build().await?;
        let address_type = cosmos.get_address_type();
        let wallet = opt.lvn_rewards_wallet.for_chain(address_type);

        let contract = Contract::new(cosmos.clone(), opt.lvn_rewards_address);

        Ok(Self {
            cosmos,
            wallet,
            contract,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let Cmd {
        opt: global_opt,
        subcommand,
    }: Cmd = Cmd::parse();

    global_opt.init_logger();

    match subcommand {
        /*  This test covers egg hatching and reward distribution. The process uses IBC messaging
           spanning three chains.

           1. Hatching dragon eggs on juno
           2. Minting NFTs on stargaze
           3. Rewarding users with LVN tokens on osmosis
        */
        Subcommand::HatchEgg { hatch_egg_opt: opt } => {
            let hatch = Hatch::new(&opt).await?;
            let nft_mint = NftMint::new(&opt).await?;
            let rewards = Rewards::new(&opt).await?;

            // Hatch the egg or retry hatching if the process started

            let hatch_status: MaybeHatchStatusResp = hatch
                .contract
                .query(HatchQueryMsg::HatchStatusByOwner {
                    owner: hatch.wallet.address().to_string().into(),
                    details: false,
                })
                .await?;

            let resp = if let Some(hatch_status) = hatch_status.resp {
                log::info!(
                    "re-hatching hatch id {} over ibc channel: {:#?}",
                    hatch_status.id,
                    hatch.config.nft_mint_channel.unwrap()
                );
                hatch
                    .contract
                    .execute(
                        &hatch.wallet,
                        vec![],
                        HatchExecMsg::RetryHatch {
                            id: hatch_status.id,
                        },
                    )
                    .await?
            } else {
                let token_id = Utc::now().timestamp_millis().to_string();
                let spirit_level: Number = "1.23".parse().unwrap();

                // mint the egg NFT

                log::info!(
                    "minting mock nft egg w/ id {} and spirit level {}",
                    token_id,
                    spirit_level
                );

                hatch
                    .burn_egg_contract
                    .execute(
                        &hatch.nft_admin_wallet,
                        vec![],
                        mock_nft::ExecuteMsg::Mint(Box::new(mock_nft::MintMsg {
                            token_id: token_id.clone(),
                            owner: hatch.wallet.address().to_string(),
                            token_uri: None,
                            extension: Metadata::new_egg(spirit_level),
                        })),
                    )
                    .await?;

                let nft_info: mock_nft::AllNftInfoResponse = hatch
                    .burn_egg_contract
                    .query(mock_nft::QueryMsg::AllNftInfo {
                        token_id: token_id.clone(),
                        include_expired: None,
                    })
                    .await?;

                // make sure the owner is correct
                assert_eq!(nft_info.access.owner, hatch.wallet.address().to_string());

                // make sure the minted nft has the correct spirit level
                let spirit_level_attr: Number = nft_info
                    .info
                    .extension
                    .attributes
                    .iter()
                    .find_map(|a| {
                        if a.trait_type == "Spirit Level" {
                            Some(a.value.parse().unwrap())
                        } else {
                            None
                        }
                    })
                    .unwrap();

                assert_eq!(spirit_level, spirit_level_attr);

                log::info!(
                    "minted mock nft egg w/ id {} and spirit level {}",
                    token_id,
                    spirit_level
                );
                log::info!(
                    "hatching nft over ibc channel: {:#?}",
                    hatch.config.nft_mint_channel.unwrap()
                );

                // hatch the egg

                hatch
                    .contract
                    .execute(
                        &hatch.wallet,
                        vec![],
                        HatchExecMsg::Hatch {
                            nft_mint_owner: nft_mint.wallet.address().to_string(),
                            eggs: vec![token_id.clone()],
                            dusts: vec![],
                        },
                    )
                    .await?
            };

            // extract our token id from the hatch event, whether re-try or first time
            let hatch_event = resp
                .events
                .iter()
                .find(|e| e.r#type == "wasm-hatch-start" || e.r#type == "wasm-hatch-retry")
                .unwrap();
            let token_id = hatch_event
                .attributes
                .iter()
                .find_map(|attr| {
                    if attr.key == "egg-token-id-0" {
                        Some(String::from_utf8(attr.value.to_vec()).unwrap())
                    } else {
                        None
                    }
                })
                .unwrap();

            let mut mint_success = false;
            let mut reward_success = false;

            loop {
                if !mint_success {
                    log::info!("checking if token {} was minted yet...", token_id);
                    match nft_mint
                        .contract
                        .query::<mock_nft::AllNftInfoResponse>(mock_nft::QueryMsg::AllNftInfo {
                            token_id: token_id.clone(),
                            include_expired: None,
                        })
                        .await
                    {
                        Ok(resp) => {
                            log::info!("Token was minted!");
                            log::info!("{:#?}", resp);

                            //todo maybe check ownership of NFT on minting chain (aka stargaze)

                            mint_success = true
                        }
                        Err(_) => {
                            log::info!("Token not minted yet");
                        }
                    }
                }

                if !reward_success {
                    log::info!("checking if LVN rewards have been transferred to rewards contract and recipient...");
                    match rewards
                        .contract
                        .query::<Option<RewardsInfoResp>>(RewardsInfo {
                            addr: rewards.wallet.to_string().into(),
                        })
                        .await
                    {
                        Ok(resp) => {
                            match resp {
                                None => {
                                    log::info!("No rewards found yet");
                                }
                                Some(resp) => {
                                    log::info!("Rewards found for {}, {:?}", rewards.wallet, resp);

                                    // After confirming the rewards contract has received the rewards,
                                    // check the recipient to see if they've received the portion that's
                                    // immediately transferred

                                    let balances = rewards
                                        .cosmos
                                        .all_balances(rewards.wallet.to_string())
                                        .await?;

                                    let amount = balances
                                        .iter()
                                        .find_map(|coin| {
                                            if coin.denom == opt.reward_token_denom {
                                                coin.amount.parse::<u128>().ok()
                                            } else {
                                                None
                                            }
                                        })
                                        .unwrap_or_default();

                                    assert!(amount > 0);

                                    reward_success = true;
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Error querying rewards contract {}", e)
                        }
                    }
                }

                if mint_success && reward_success {
                    break;
                } else {
                    tokio::time::sleep(tokio::time::Duration::from_secs(opt.ibc_sleep_seconds))
                        .await;
                }
            }
        }
    }

    Ok(())
}
