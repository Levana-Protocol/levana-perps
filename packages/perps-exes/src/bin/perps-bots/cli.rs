use std::{net::SocketAddr, str::FromStr};

use anyhow::{Context, Result};
use cosmos::{AddressType, SeedPhrase, Wallet};
use perps_exes::build_version;

#[derive(clap::Parser)]
#[clap(version = build_version())]
pub(crate) struct Opt {
    #[clap(long, short)]
    verbose: bool,
    #[clap(long, default_value = "0.0.0.0:3000", env = "LEVANA_BOTS_BIND")]
    pub(crate) bind: SocketAddr,
    /// Deployment name to use (aka contract family)
    #[clap(long, env = "LEVANA_BOTS_DEPLOYMENT")]
    pub(crate) deployment: String,
    /// Override the gRPC URL
    #[clap(long, env = "COSMOS_GRPC")]
    pub(crate) grpc_url: Option<String>,
    /// Override the RPC URL
    #[clap(long, env = "COSMOS_RPC")]
    pub(crate) rpc_url: Option<String>,
    /// hCaptcha secret key
    #[clap(long, env = "LEVANA_BOTS_HCAPTCHA_SECRET")]
    pub(crate) hcaptcha_secret: String,
    /// Maintenance mode to use. Empty string is treated as no maintenance mode.
    #[clap(long, env = "LEVANA_BOTS_MAINTENANCE")]
    pub(crate) maintenance: Option<String>,
    /// Override the number of trading bots to run
    #[clap(long, env = "LEVANA_BOTS_TRADERS")]
    pub(crate) traders: Option<usize>,
    /// Override the contents of the DeploymentConfig in YAML format
    #[clap(long, env = "LEVANA_BOTS_DEPLOYMENT_CONFIG")]
    pub(crate) deployment_config: Option<String>,
}

impl Opt {
    pub(crate) fn init_logger(&self) {
        let env = env_logger::Env::default().default_filter_or(if self.verbose {
            format!(
                "{}=debug,cosmos=debug,levana=debug,info",
                env!("CARGO_CRATE_NAME")
            )
        } else {
            "info".to_owned()
        });
        env_logger::Builder::from_env(env).init();
    }

    pub(crate) fn get_wallet(
        &self,
        address_type: AddressType,
        wallet_phrase_name: &str,
        wallet_type: &str,
    ) -> Result<Wallet> {
        let env_var = format!("LEVANA_BOTS_PHRASE_{wallet_phrase_name}_{wallet_type}");
        let phrase = get_env(&env_var)?;
        let wallet = Wallet::from_phrase(&phrase, address_type)?;
        log::info!("Wallet address for {wallet_type}: {wallet}");
        Ok(wallet)
    }

    pub(crate) fn get_wallet_seed(
        &self,
        wallet_phrase_name: &str,
        wallet_type: &str,
    ) -> Result<SeedPhrase> {
        let env_var = format!("LEVANA_BOTS_PHRASE_{wallet_phrase_name}_{wallet_type}");
        let phrase = get_env(&env_var)?;
        SeedPhrase::from_str(&phrase)
    }

    pub(crate) fn get_faucet_bot_wallet(&self, address_type: AddressType) -> Result<Wallet> {
        let env_var = "LEVANA_BOTS_PHRASE_FAUCET";
        let phrase = get_env(env_var)?;
        let wallet = Wallet::from_phrase(&phrase, address_type)?;
        log::info!("Wallet address for faucet: {wallet}");
        Ok(wallet)
    }

    /// One shared wallet used for refilling gas to all other wallets.
    pub(crate) fn get_gas_wallet(&self, address_type: AddressType) -> Result<Wallet> {
        let env_var = "LEVANA_BOTS_PHRASE_GAS";
        let phrase = get_env(env_var)?;
        let wallet = Wallet::from_phrase(&phrase, address_type)?;
        log::info!("Wallet address for gas: {wallet}");
        Ok(wallet)
    }

    pub(crate) fn get_crank_wallet(
        &self,
        address_type: AddressType,
        wallet_phrase_name: &str,
        index: u32,
    ) -> Result<Wallet> {
        let env_var = format!("LEVANA_BOTS_PHRASE_{}_CRANK", wallet_phrase_name);
        let phrase = get_env(&env_var)?;
        let seed = SeedPhrase::from_str(&phrase)?;
        seed.derive_cosmos_numbered(index).map(|x| {
            let wallet = x.for_chain(address_type);
            log::info!("Crank bot wallet: {wallet}");
            wallet
        })
    }
}

fn get_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Unable to load enviornment variable {key}"))
}
