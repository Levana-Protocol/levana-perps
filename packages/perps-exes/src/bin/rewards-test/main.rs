mod cli;
mod mock_nft;

use std::vec;

use crate::cli::{Cmd, Subcommand};
use anyhow::Result;
use clap::Parser;
use cli::HatchEggOpt;
use cosmos::{Contract, Cosmos, HasAddressType, Wallet};
use mock_nft::Metadata;
use msg::contracts::hatching::{
    config::Config as HatchConfig,
    entry::{ExecuteMsg as HatchExecMsg, MaybeHatchStatusResp, QueryMsg as HatchQueryMsg},
    NftRarity,
};
use msg::contracts::rewards::entry::ExecuteMsg::Claim;
use msg::contracts::rewards::entry::{QueryMsg::RewardsInfo, RewardsInfoResp};
use perps_exes::prelude::*;
use serde::{Deserialize, Serialize};

struct Hatch {
    #[allow(dead_code)]
    pub cosmos: Cosmos,
    pub wallet: Wallet,
    pub nft_mint_admin_wallet: Wallet,
    pub profile_admin_wallet: Wallet,
    pub contract: Contract,
    pub burn_egg_contract: Contract,
    pub profile_contract: Contract,
    pub config: HatchConfig,
}

impl Hatch {
    pub async fn new(opt: &HatchEggOpt) -> Result<Self> {
        let cosmos = opt.hatch_network.builder().build().await?;
        let address_type = cosmos.get_address_type();
        let wallet = opt.hatch_wallet.for_chain(address_type);
        let nft_mint_admin_wallet = opt.nft_mint_admin_wallet.for_chain(address_type);
        let profile_admin_wallet = opt.profile_admin_wallet.for_chain(address_type);

        let contract = Contract::new(cosmos.clone(), opt.hatch_address);

        let config: HatchConfig = contract.query(HatchQueryMsg::Config {}).await?;

        let burn_egg_contract = Contract::new(
            cosmos.clone(),
            config.nft_burn_contracts.egg.to_string().parse().unwrap(),
        );

        let profile_contract = Contract::new(
            cosmos.clone(),
            config.profile_contract.to_string().parse().unwrap(),
        );

        Ok(Self {
            cosmos,
            wallet,
            nft_mint_admin_wallet,
            profile_admin_wallet,
            contract,
            burn_egg_contract,
            profile_contract,
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

async fn get_lvn_balance(rewards: &Rewards, denom: &String) -> Result<u128> {
    let balances = rewards
        .cosmos
        .all_balances(rewards.wallet.address())
        .await?;

    let amount = balances
        .iter()
        .find_map(|coin| {
            if coin.denom == *denom {
                coin.amount.parse::<u128>().ok()
            } else {
                None
            }
        })
        .unwrap_or_default();

    Ok(amount)
}

#[tokio::main]
async fn main() -> Result<()> {
    let Cmd {
        opt: global_opt,
        subcommand,
    }: Cmd = Cmd::parse();

    global_opt.init_logger();

    match subcommand {
        /*  This test covers egg hatching and reward grants. The process uses IBC messaging
           spanning three chains.

           1. Hatching dragon eggs on juno
           2. Minting NFTs on stargaze
           3. Rewarding users with LVN tokens on osmosis
        */
        Subcommand::HatchEgg { hatch_egg_opt: opt } => {
            let hatch = Hatch::new(&opt).await?;
            let nft_mint = NftMint::new(&opt).await?;
            let rewards = Rewards::new(&opt).await?;
            let lvn_balance_before = get_lvn_balance(&rewards, &opt.reward_token_denom).await?;

            // Clear out pre-existing lvn rewards
            clear_lvn_rewards(&rewards).await?;

            // Retry hatching if the process started, or start a new hatch
            let resp = if let Some(hatch_status) = get_hatch_status(&hatch, false).await?.resp {
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
                add_profile_spirit_level(&hatch, "1.23".parse().unwrap()).await?;
                let egg_token_id =
                    mint_egg(&hatch, "1.23".parse().unwrap(), NftRarity::Ancient).await?;

                // hatch the egg

                let tx = hatch
                    .contract
                    .execute(
                        &hatch.wallet,
                        vec![],
                        HatchExecMsg::Hatch {
                            nft_mint_owner: nft_mint.wallet.address().to_string(),
                            lvn_grant_address: rewards.wallet.address().to_string(),
                            profile: true,
                            eggs: vec![egg_token_id],
                            dusts: vec![],
                        },
                    )
                    .await?;

                // confirm that we have the correct LVN for eggs and profile
                let details = get_hatch_status(&hatch, true)
                    .await?
                    .resp
                    .unwrap()
                    .status
                    .details
                    .unwrap();

                assert_eq!(details.eggs.get(0).unwrap().lvn, "3.5547".parse().unwrap());
                // we've some spirit level, but there might have been some previously somehow
                assert!(details.profile.unwrap().lvn >= "2.9643".parse().unwrap());

                tx
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
                            addr: rewards.wallet.address().to_string().into(),
                        })
                        .await
                    {
                        Ok(resp) => {
                            match resp {
                                None => {
                                    log::info!("No rewards found yet");
                                }
                                Some(resp) => {
                                    log::info!(
                                        "Rewards found for {}, {:#?}",
                                        rewards.wallet.address(),
                                        resp
                                    );

                                    // After confirming the rewards contract has received the rewards,
                                    // check the recipient to see if they've received the portion that's
                                    // immediately transferred

                                    let lvn_balance_after =
                                        get_lvn_balance(&rewards, &opt.reward_token_denom).await?;
                                    let diff = lvn_balance_after - lvn_balance_before;
                                    assert!(
                                        diff > 0,
                                        "lvn balance before: {}, lvn balance after: {}",
                                        lvn_balance_before,
                                        lvn_balance_after
                                    );

                                    log::info!("recipient received {} lvn tokens", diff);
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

async fn clear_lvn_rewards(rewards: &Rewards) -> Result<()> {
    log::info!("Clearing our rewards for {}...", rewards.wallet.address());
    loop {
        let res = rewards
            .contract
            .query::<Option<RewardsInfoResp>>(RewardsInfo {
                addr: rewards.wallet.address().to_string().into(),
            })
            .await?;

        match res {
            None => {
                log::info!("...rewards are clear");
                break;
            }
            Some(info) => {
                if info.unlocked.is_zero() {
                    log::info!("... no unlocked rewards");
                    break;
                } else {
                    log::info!("...found {:?} rewards, claiming...", info);
                    rewards
                        .contract
                        .execute(&rewards.wallet, vec![], Claim {})
                        .await?;
                }
            }
        }

        // hardcoding sleep to 10 seconds since that's what `ConfigUpdate.unlock_duration_seconds`
        // is set to when deploying the test rewards contract
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }

    Ok(())
}

async fn get_hatch_status(hatch: &Hatch, details: bool) -> Result<MaybeHatchStatusResp> {
    hatch
        .contract
        .query(HatchQueryMsg::HatchStatusByOwner {
            owner: hatch.wallet.address().to_string().into(),
            details,
        })
        .await
}

async fn mint_egg(hatch: &Hatch, spirit_level: NumberGtZero, rarity: NftRarity) -> Result<String> {
    // a Wyvern egg
    let token_id = "4".to_string();
    // mint the egg NFT

    log::info!(
        "minting mock nft egg w/ id {} and spirit level {}",
        token_id,
        spirit_level
    );

    hatch
        .burn_egg_contract
        .execute(
            &hatch.nft_mint_admin_wallet,
            vec![],
            mock_nft::ExecuteMsg::Mint(Box::new(mock_nft::MintMsg {
                token_id: token_id.clone(),
                owner: hatch.wallet.address().to_string(),
                token_uri: None,
                extension: Metadata::new_egg(spirit_level, rarity),
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
    let spirit_level_attr: NumberGtZero = nft_info
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
        hatch.config.nft_mint_channel.as_ref().unwrap()
    );

    Ok(token_id)
}

async fn add_profile_spirit_level(hatch: &Hatch, spirit_level: NumberGtZero) -> Result<()> {
    log::info!("adding profile spirit level {}", spirit_level);

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum ProfileExecuteMsg {
        Admin { message: ProfileAdminExecuteMsg },
    }
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ProfileAdminExecuteMsg {
        AddSpiritLevel { wallets: Vec<AddSpiritLevel> },
    }
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub struct AddSpiritLevel {
        pub wallet: String,
        pub spirit_level: String,
    }

    hatch
        .profile_contract
        .execute(
            &hatch.profile_admin_wallet,
            vec![],
            &ProfileExecuteMsg::Admin {
                message: ProfileAdminExecuteMsg::AddSpiritLevel {
                    wallets: vec![AddSpiritLevel {
                        wallet: hatch.wallet.address().to_string(),
                        spirit_level: spirit_level.to_string(),
                    }],
                },
            },
        )
        .await?;

    Ok(())
}
