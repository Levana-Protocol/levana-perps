use cosmos::{Address, CosmosNetwork, RawWallet};
use perps_exes::build_version;

#[derive(clap::Parser)]
#[clap(version = build_version())]
pub(crate) struct Cmd {
    #[clap(flatten)]
    pub opt: Opt,
    #[clap(subcommand)]
    pub(crate) subcommand: Subcommand,
}

#[derive(clap::Parser)]
pub(crate) enum Subcommand {
    /// Hatch and egg and see that we get the correct rewards
    HatchEgg {
        #[clap(flatten)]
        hatch_egg_opt: HatchEggOpt,
    },
}

#[derive(clap::Parser, Clone)]
pub(crate) struct Opt {
    /// Turn on verbose logging
    #[clap(long, short, global = true)]
    verbose: bool,
}

#[derive(clap::Parser)]
pub(crate) struct HatchEggOpt {
    /// Mnemonic phrase for the hatching wallet
    #[clap(long, env = "MOCK_NFT_ADMIN_COSMOS_WALLET")]
    pub mock_nft_admin_wallet: RawWallet,

    /// Network to use for hatching
    #[clap(long, env = "HATCH_COSMOS_NETWORK")]
    pub hatch_network: CosmosNetwork,

    /// Mnemonic phrase for the hatching wallet
    #[clap(long, env = "HATCH_COSMOS_WALLET")]
    pub hatch_wallet: RawWallet,

    /// Network to use for the minted nft rewards
    #[clap(long, env = "NFT_MINT_COSMOS_NETWORK")]
    pub nft_mint_network: CosmosNetwork,

    /// Mnemonic phrase for the minted nft rewards wallet
    #[clap(long, env = "NFT_MINT_COSMOS_WALLET")]
    pub nft_mint_wallet: RawWallet,

    /// Network to use for LVN rewards
    #[clap(long, env = "LVN_REWARDS_COSMOS_NETWORK")]
    pub lvn_rewards_network: CosmosNetwork,

    /// Mnemonic phrase for the lvn rewards wallet
    #[clap(long, env = "LVN_REWARDS_COSMOS_WALLET")]
    pub lvn_rewards_wallet: RawWallet,

    /// hatching contract address
    #[clap(
        long,
        env = "HATCH_ADDRESS",
        default_value = "juno1tuz9em4t0uhg794rt0c8dl28p6e9np8phytmvhas4mfvqv0ph79qjkh6dt"
    )]
    pub hatch_address: Address,

    // this is the address for the NFT minting itself (i.e. Levana Baby Dragons), not the ibc execute proxy contract
    #[clap(
        long,
        env = "NFT_MINT_ADDRESS",
        default_value = "stars126ucy5awfnqx909mevlz6x4r69u5e9ekhjrvkrypfnap747jvlyqcw80wv"
    )]
    pub nft_mint_address: Address,

    /// LVN Rewards contract address
    #[clap(
        long,
        env = "LVN_REWARDS_ADDRESS",
        default_value = "osmo14emhtww4j2xfudqfnjez7gn46tu0jwuuw55lqheq6hcetpsvh7qqh7lcu0"
    )]
    pub lvn_rewards_address: Address,

    // Amount of time to sleep before checking for ibc updates
    #[clap(long, default_value = "5")]
    pub ibc_sleep_seconds: u64,

    /// Reward token denom
    #[clap(long, env = "REWARD_TOKEN_DENOM")]
    pub reward_token_denom: String,
}

impl Opt {
    pub(crate) fn init_logger(&self) {
        let env = env_logger::Env::default().default_filter_or(if self.verbose {
            format!("{}=debug,cosmos=debug,info", env!("CARGO_CRATE_NAME"))
        } else {
            "info".to_owned()
        });
        env_logger::Builder::from_env(env).init();
    }
}
