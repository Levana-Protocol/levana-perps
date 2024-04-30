#![deny(clippy::as_conversions)]

mod cli;
mod mock_nft;

use std::{path::Path, vec};

use crate::cli::{Cmd, Subcommand};
use anyhow::Result;
use clap::Parser;
use cli::HatchEggOpt;
use cosmos::{Address, Contract, Cosmos, HasAddress, HasAddressHrp, SeedPhrase, Wallet};
use mock_nft::Metadata;
use msg::contracts::hatching::{
    config::Config as HatchConfig,
    dragon_mint::DragonMintExtra,
    entry::{
        ExecuteMsg as HatchExecMsg, MaybeHatchStatusResp, PotentialHatchInfo,
        QueryMsg as HatchQueryMsg,
    },
    NftRarity,
};
use msg::contracts::rewards::entry::ExecuteMsg::Claim;
use msg::contracts::rewards::entry::{QueryMsg::RewardsInfo, RewardsInfoResp};
use perps_exes::{prelude::*, PerpsNetwork};
use serde::{Deserialize, Serialize};

struct Hatch {
    #[allow(dead_code)]
    pub cosmos: Cosmos,
    pub wallet: Wallet,
    pub nft_mint_admin_wallet: Wallet,
    pub profile_admin_wallet: Wallet,
    pub contract: Contract,
    pub burn_egg_contract: Contract,
    pub burn_dust_contract: Contract,
    pub profile_contract: Contract,
    pub config: HatchConfig,
}

impl Hatch {
    pub async fn new(
        network: PerpsNetwork,
        wallet: SeedPhrase,
        nft_mint_admin_wallet: SeedPhrase,
        profile_admin_wallet: SeedPhrase,
        hatch_address: Address,
    ) -> Result<Self> {
        let cosmos = network.builder().await?.build().await?;
        let address_type = cosmos.get_address_hrp();
        let wallet = wallet.with_hrp(address_type)?;
        let nft_mint_admin_wallet = nft_mint_admin_wallet.with_hrp(address_type)?;
        let profile_admin_wallet = profile_admin_wallet.with_hrp(address_type)?;

        let contract = cosmos.make_contract(hatch_address);

        let config: HatchConfig = contract.query(HatchQueryMsg::Config {}).await?;

        let burn_egg_contract =
            cosmos.make_contract(config.nft_burn_contracts.egg.to_string().parse().unwrap());

        let burn_dust_contract =
            cosmos.make_contract(config.nft_burn_contracts.dust.to_string().parse().unwrap());

        let profile_contract =
            cosmos.make_contract(config.profile_contract.to_string().parse().unwrap());

        Ok(Self {
            cosmos,
            wallet,
            nft_mint_admin_wallet,
            profile_admin_wallet,
            contract,
            burn_egg_contract,
            burn_dust_contract,
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
        let cosmos = opt.nft_mint_network.builder().await?.build().await?;
        let address_type = cosmos.get_address_hrp();
        let wallet = opt.nft_mint_wallet.with_hrp(address_type)?;

        let contract = cosmos.make_contract(opt.nft_mint_address);

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
        let cosmos = opt.lvn_rewards_network.builder().await?.build().await?;
        let address_type = cosmos.get_address_hrp();
        let wallet = opt.lvn_rewards_wallet.with_hrp(address_type)?;

        let contract = cosmos.make_contract(opt.lvn_rewards_address);

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
        .all_balances(rewards.wallet.get_address())
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
        Subcommand::MintTest { mint_test_opt: opt } => {
            let hatch = Hatch::new(
                opt.hatch_network,
                opt.hatch_wallet,
                opt.nft_mint_admin_wallet,
                opt.profile_admin_wallet,
                opt.hatch_address,
            )
            .await?;

            let info = mint_test(
                &hatch,
                opt.owner.to_string(),
                &opt.path_to_hatchery,
                opt.mint_eggs_start_skip,
                opt.mint_eggs_count,
                opt.mint_dusts_count,
                opt.profile_spirit_level,
                opt.egg_spirit_level,
                opt.dust_spirit_level,
            )
            .await?;

            println!("{:#?}", info);
        }

        /*  This test covers egg hatching and reward grants. The process uses IBC messaging
           spanning three chains.

           1. Hatching dragon eggs on juno
           2. Minting NFTs on stargaze
           3. Rewarding users with LVN tokens on osmosis
        */
        Subcommand::HatchEgg { hatch_egg_opt: opt } => {
            let hatch = Hatch::new(
                opt.hatch_network,
                opt.hatch_wallet.clone(),
                opt.nft_mint_admin_wallet.clone(),
                opt.profile_admin_wallet.clone(),
                opt.hatch_address,
            )
            .await?;

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
                    hatch.config.nft_mint_channel.as_ref().unwrap()
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
                let info = mint_test(
                    &hatch,
                    hatch.wallet.get_address_string(),
                    &opt.path_to_hatchery,
                    opt.mint_eggs_start_skip,
                    opt.mint_eggs_count,
                    opt.mint_dusts_count,
                    Some("1.23".parse().unwrap()),
                    "1.23".parse().unwrap(),
                    "1.23".parse().unwrap(),
                )
                .await?;

                log::info!(
                    "hatching nft over ibc channel: {:#?}",
                    hatch.config.nft_mint_channel.as_ref().unwrap()
                );

                let eggs = info
                    .eggs
                    .iter()
                    .map(|egg| egg.token_id.to_string())
                    .collect::<Vec<_>>();

                // hatch everything
                let tx = hatch
                    .contract
                    .execute(
                        &hatch.wallet,
                        vec![],
                        HatchExecMsg::Hatch {
                            nft_mint_owner: nft_mint.wallet.get_address_string(),
                            lvn_grant_address: rewards.wallet.get_address_string(),
                            profile: true,
                            eggs,
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

                assert_eq!(details.eggs.first().unwrap().lvn, "3.5547".parse().unwrap());
                // we've some spirit level, but there might have been some previously somehow
                assert!(details.profile.unwrap().lvn >= "2.9643".parse().unwrap());

                tx
            };

            // extract our token id from the hatch event, whether re-try or first time
            let hatch_event = resp
                .events
                .iter()
                .find(|e| {
                    e.r#type.starts_with("wasm-hatch-start")
                        || e.r#type.starts_with("wasm-hatch-retry")
                })
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
                        Ok(_) => {
                            log::info!("Token was minted!");
                            //todo maybe check ownership of NFT on minting chain (aka stargaze)

                            mint_success = true
                        }
                        Err(_) => {
                            log::info!("Token not minted yet");
                        }
                    }
                }

                if !reward_success {
                    match rewards
                        .contract
                        .query::<Option<RewardsInfoResp>>(RewardsInfo {
                            addr: rewards.wallet.get_address_string().into(),
                        })
                        .await
                    {
                        Ok(resp) => {
                            match resp {
                                None => {
                                    log::info!("No rewards found yet");
                                }
                                Some(resp) => {
                                    log::info!("Rewards found for {}, {:#?}", rewards.wallet, resp);

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
                    let resp = get_hatch_status(&hatch, true).await?.resp.unwrap();

                    log::info!("hatch {} complete!", resp.id);
                    log::info!("{:#?}", resp.status);
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
    log::info!("Clearing our rewards for {}...", rewards.wallet);
    loop {
        let res = rewards
            .contract
            .query::<Option<RewardsInfoResp>>(RewardsInfo {
                addr: rewards.wallet.get_address_string().into(),
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

async fn get_hatch_status(hatch: &Hatch, details: bool) -> cosmos::Result<MaybeHatchStatusResp> {
    hatch
        .contract
        .query(HatchQueryMsg::HatchStatusByOwner {
            owner: hatch.wallet.get_address_string().into(),
            details,
        })
        .await
}

#[allow(clippy::too_many_arguments)]
async fn mint_test(
    hatch: &Hatch,
    owner: String,
    path_to_hatchery: &Path,
    mint_eggs_start_skip: usize,
    mint_eggs_count: u32,
    mint_dusts_count: u32,
    profile_spirit_level: Option<NumberGtZero>,
    egg_spirit_level: NumberGtZero,
    dust_spirit_level: NumberGtZero,
) -> Result<PotentialHatchInfo> {
    if let Some(profile_spirit_level) = profile_spirit_level {
        add_profile_spirit_level(hatch, profile_spirit_level, owner.clone()).await?;
    }
    let filepath = path_to_hatchery
        .join("data")
        .join("juno-warp")
        .join("baby-dragons-extra.csv");
    let mut rdr = csv::Reader::from_path(filepath)?;
    let dragon_extras: Vec<DragonMintExtra> = rdr
        .deserialize::<DragonMintExtra>()
        .map(|x| x.map_err(|e| e.into()))
        .collect::<Result<Vec<_>>>()?;

    let eggs = mint_eggs(
        hatch,
        owner.clone(),
        mint_eggs_start_skip,
        mint_eggs_count,
        egg_spirit_level,
        NftRarity::Ancient,
        dragon_extras,
    )
    .await?;

    let dusts = mint_dusts(
        hatch,
        owner.clone(),
        mint_dusts_count,
        dust_spirit_level,
        NftRarity::Ancient,
    )
    .await?;
    // query for the "potential hatch info"

    let info: PotentialHatchInfo = hatch
        .contract
        .query(&HatchQueryMsg::PotentialHatchInfo {
            owner: owner.clone().into(),
            eggs: eggs.clone(),
            dusts: dusts.clone(),
            profile: true,
        })
        .await?;

    assert_eq!(info.eggs.len(), eggs.len());
    assert_eq!(info.dusts.len(), dusts.len());

    Ok(info)
}
async fn mint_eggs(
    hatch: &Hatch,
    owner: String,
    mint_eggs_start_skip: usize,
    mint_eggs_count: u32,
    spirit_level: NumberGtZero,
    rarity: NftRarity,
    dragon_extras: Vec<DragonMintExtra>,
) -> Result<Vec<String>> {
    // mint the egg NFT

    let mut token_ids = vec![];
    let mut skip_offset = mint_eggs_start_skip;

    for _ in 0..mint_eggs_count {
        let mut token_id = None;
        for dragon_extra in dragon_extras.iter().skip(skip_offset) {
            skip_offset += 1;
            log::info!(
                "minting mock nft egg w/ id {} and spirit level {}",
                dragon_extra.id,
                spirit_level
            );

            let res = hatch
                .burn_egg_contract
                .execute(
                    &hatch.nft_mint_admin_wallet,
                    vec![],
                    mock_nft::ExecuteMsg::Mint(Box::new(mock_nft::MintMsg {
                        token_id: dragon_extra.id.clone(),
                        owner: owner.clone(),
                        token_uri: None,
                        extension: Metadata::new_egg(
                            dragon_extra.id.clone(),
                            spirit_level,
                            rarity,
                            dragon_extra.kind.clone(),
                        ),
                    })),
                )
                .await;

            match res {
                Ok(_) => {
                    token_id = Some(dragon_extra.id.clone());
                    break;
                }
                Err(err) => {
                    let s = err.to_string();
                    if s.contains("remint a token") || s.contains("already claimed") {
                        log::warn!(
                            "token {} was already minted or claimed, trying the next one...",
                            dragon_extra.id
                        );
                    } else {
                        return Err(err.into());
                    }
                }
            }
        }

        let token_id = token_id.context("no token id found")?;

        let nft_info: mock_nft::AllNftInfoResponse = hatch
            .burn_egg_contract
            .query(mock_nft::QueryMsg::AllNftInfo {
                token_id: token_id.clone(),
                include_expired: None,
            })
            .await?;

        // make sure the owner is correct
        assert_eq!(nft_info.access.owner, owner);

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
        token_ids.push(token_id);
    }

    Ok(token_ids)
}

async fn mint_dusts(
    hatch: &Hatch,
    owner: String,
    mint_dusts_count: u32,
    spirit_level: NumberGtZero,
    rarity: NftRarity,
) -> Result<Vec<String>> {
    // mint the dust NFT

    let mut token_ids = vec![];

    let now: u64 = chrono::Utc::now().timestamp().try_into()?;

    for i in 0..mint_dusts_count {
        let token_id = format!("{}", now + u64::from(i));
        log::info!(
            "minting mock dust nft w/ id {} and spirit level {}",
            token_id,
            spirit_level
        );

        hatch
            .burn_dust_contract
            .execute(
                &hatch.nft_mint_admin_wallet,
                vec![],
                mock_nft::ExecuteMsg::Mint(Box::new(mock_nft::MintMsg {
                    token_id: token_id.clone(),
                    owner: owner.clone(),
                    token_uri: None,
                    extension: Metadata::new_dust(spirit_level, rarity),
                })),
            )
            .await?;

        let nft_info: mock_nft::AllNftInfoResponse = hatch
            .burn_dust_contract
            .query(mock_nft::QueryMsg::AllNftInfo {
                token_id: token_id.clone(),
                include_expired: None,
            })
            .await?;

        // make sure the owner is correct
        assert_eq!(nft_info.access.owner, owner);

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
            "minted mock nft dust w/ id {} and spirit level {}",
            token_id,
            spirit_level
        );
        token_ids.push(token_id);
    }

    Ok(token_ids)
}

async fn add_profile_spirit_level(
    hatch: &Hatch,
    spirit_level: NumberGtZero,
    owner: String,
) -> Result<()> {
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
                        wallet: owner,
                        spirit_level: spirit_level.to_string(),
                    }],
                },
            },
        )
        .await?;

    Ok(())
}
