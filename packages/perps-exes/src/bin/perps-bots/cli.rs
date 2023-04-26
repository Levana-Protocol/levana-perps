use std::{net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::{Context, Result};
use cosmos::{AddressType, CosmosNetwork, HasAddressType, SeedPhrase, Wallet};
use perps_exes::build_version;
use perps_exes::config::{ChainConfig, Config, DeploymentConfig};
use perps_exes::wallet_manager::WalletManager;

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
    /// hCaptcha secret key
    #[clap(long, env = "LEVANA_BOTS_HCAPTCHA_SECRET")]
    pub(crate) hcaptcha_secret: String,
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
    ) -> Result<Wallet> {
        let env_var = format!("LEVANA_BOTS_PHRASE_{}_CRANK", wallet_phrase_name);
        let phrase = get_env(&env_var)?;
        let seed = SeedPhrase::from_str(&phrase)?;
        seed.derive_cosmos().map(|x| {
            let wallet = x.for_chain(address_type);
            log::info!("Crank bot wallet: {wallet}");
            wallet
        })
    }

    pub(crate) fn parse_deployment(&self) -> Result<(CosmosNetwork, &str)> {
        const NETWORKS: &[(CosmosNetwork, &str)] = &[
            (CosmosNetwork::OsmosisTestnet, "osmo"),
            (CosmosNetwork::Dragonfire, "dragon"),
        ];
        for (network, prefix) in NETWORKS {
            if let Some(suffix) = self.deployment.strip_prefix(prefix) {
                return Ok((*network, suffix));
            }
        }
        Err(anyhow::anyhow!(
            "Could not parse deployment: {}",
            self.deployment
        ))
    }
}

fn get_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Unable to load enviornment variable {key}"))
}

pub(crate) fn get_deployment_config(
    config: &'static Config,
    opt: &Opt,
) -> Result<DeploymentConfig> {
    let (network, suffix) = opt.parse_deployment()?;
    let wallet_phrase_name = suffix.to_ascii_uppercase();
    let partial_config = config
        .deployments
        .get(suffix)
        .with_context(|| {
            format!(
                "No config found for {}. Valid configs: {}",
                suffix,
                config
                    .deployments
                    .keys()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?
        .clone();
    let ChainConfig {
        tracker,
        faucet,
        explorer,
        pyth,
        watcher,
    } = config
        .chains
        .get(&network)
        .with_context(|| format!("No chain config found for network {}", network))?;
    Ok(DeploymentConfig {
        tracker: *tracker,
        faucet: *faucet,
        pyth: pyth.clone(),
        min_gas: config.min_gas,
        min_gas_in_faucet: config.min_gas_in_faucet,
        min_gas_in_gas_wallet: config.min_gas_in_gas_wallet,
        price_api: &config.price_api,
        explorer,
        contract_family: opt.deployment.clone(),
        network,
        crank_wallet: opt.get_crank_wallet(network.get_address_type(), &wallet_phrase_name)?,
        price_wallet: if partial_config.price {
            Some(Arc::new(opt.get_wallet(
                network.get_address_type(),
                &wallet_phrase_name,
                "PRICE",
            )?))
        } else {
            None
        },
        wallet_manager: WalletManager::new(
            opt.get_wallet_seed(&wallet_phrase_name, "WALLET_MANAGER")?,
            network.get_address_type(),
        )?,
        balance: partial_config.balance,
        liquidity: partial_config.liquidity,
        utilization: partial_config.utilization,
        traders: partial_config.traders,
        liquidity_config: config.liquidity.clone(),
        utilization_config: config.utilization,
        trader_config: config.trader,
        watcher: watcher.clone(),
    })
}
