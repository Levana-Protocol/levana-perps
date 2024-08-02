pub mod defaults;

use std::{collections::BTreeMap, fmt::Write, iter::Sum, ops::AddAssign, path::Path};

use chrono::{DateTime, Utc};
use cosmos::{Address, CosmosNetwork, RawAddress};
use cosmwasm_std::{Uint128, Uint256};
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use msg::{
    contracts::market::{
        config::{defaults::ConfigDefaults, ConfigUpdate},
        entry::InitialPrice,
        spot_price::PythPriceServiceNetwork,
    },
    prelude::*,
    token::TokenInit,
};
use pyth_sdk_cw::PriceIdentifier;

use crate::PerpsNetwork;

/// Configuration for chainwide data.
///
/// This contains information which would be valid for multiple different
/// contract deployments on a single chain.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ChainConfig {
    pub tracker: Option<Address>,
    pub faucet: Option<Address>,
    pub spot_price: Option<ChainSpotPriceConfig>,
    pub explorer: Option<String>,
    /// Potential RPC endpoints to use
    #[serde(default)]
    pub rpc_nodes: Vec<String>,
    /// Override the gas multiplier
    pub gas_multiplier: Option<f64>,
    /// Number of decimals in the gas coin
    pub gas_decimals: GasDecimals,
    #[serde(default)]
    pub assets: BTreeMap<String, NativeAsset>,
    pub age_tolerance_seconds: Option<u32>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NativeAsset {
    pub denom: String,
    pub decimal_places: u8,
}

impl From<&NativeAsset> for TokenInit {
    fn from(
        NativeAsset {
            denom,
            decimal_places,
        }: &NativeAsset,
    ) -> Self {
        TokenInit::Native {
            denom: denom.clone(),
            decimal_places: *decimal_places,
        }
    }
}

/// Spot price config for a given chain
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ChainSpotPriceConfig {
    /// Pyth configuration, required on chains that use pyth feeds
    pub pyth: Option<ChainPythConfig>,
    /// Stride configuration, required on chains that use stride
    pub stride: Option<ChainStrideConfig>,
}

/// Configuration for pyth
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ChainPythConfig {
    /// The address of the pyth oracle contract
    pub contract: Address,
    /// Which network to use for the price service
    /// This isn't used for any internal logic, but clients must use the appropriate
    /// price service endpoint to match this
    pub r#type: PythPriceServiceNetwork,
}

/// Configuration for stride
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ChainStrideConfig {
    /// The address of the redemption rate contract
    pub contract: Address,
}

/// Overall configuration of prices, for information valid across all chains.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PriceConfig {
    pub pyth: PythPriceConfig,
    /// Mappings from a key to price feed
    pub networks: BTreeMap<PerpsNetwork, BTreeMap<MarketId, MarketPriceFeedConfigs>>,
}

/// Overall configuration of Pyth, for information valid across all chains.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PythPriceConfig {
    /// Configuration for stable feeds
    pub stable: PythPriceServiceConfig,
    /// Configuration for edge feeds
    pub edge: PythPriceServiceConfig,
}

/// Overall configuration of Pyth, for information valid across all chains.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PythPriceServiceConfig {
    /// How old a price to allow, in seconds
    pub update_age_tolerance: u32,
    /// Mappings from a key to price feed  id
    pub feed_ids: BTreeMap<String, PriceIdentifier>,
}
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MarketPriceFeedConfigs {
    /// feed of the base asset in terms of the quote asset
    pub feeds: Vec<MarketPriceFeedConfig>,
    /// feed of the collateral asset in terms of USD
    pub feeds_usd: Vec<MarketPriceFeedConfig>,
    /// Override the Stride contract address for this market
    pub stride_contract: Option<Address>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum MarketPriceFeedConfig {
    Pyth {
        key: String,
        inverted: bool,
    },
    Constant {
        price: NumberGtZero,
        inverted: bool,
    },
    Sei {
        denom: String,
        inverted: bool,
    },
    Stride {
        denom: String,
        inverted: bool,
        age_tolerance: u32,
    },
    Simple {
        contract: Address,
        inverted: bool,
        age_tolerance: u32,
    },
}

/// Number of decimals in the gas coin
#[derive(
    serde::Deserialize, serde::Serialize, Clone, Debug, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct GasDecimals(pub u8);
impl GasDecimals {
    pub fn from_u128(self, raw: u128) -> Result<GasAmount> {
        Decimal256::from_atomics(raw, self.0.into())
            .with_context(|| {
                format!(
                    "GasDecimals::from_u128 failed on {raw} with {} decimals",
                    self.0
                )
            })
            .map(GasAmount)
    }

    pub fn to_u128(self, amount: GasAmount) -> Result<u128> {
        let factor = Decimal256::one().atomics() / Uint256::from_u128(10).pow(self.0.into());
        let raw = amount.0.atomics() / factor;

        Uint128::try_from(raw).map(|x| x.u128()).with_context(|| {
            format!(
                "Unable to convert gas amount {amount} to u128 with decimals {}",
                self.0
            )
        })
    }
}
impl FromStr for GasDecimals {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        s.parse().map(GasDecimals).map_err(From::from)
    }
}

#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default,
)]
pub struct GasAmount(pub Decimal256);

impl Sum<GasAmount> for GasAmount {
    fn sum<I: Iterator<Item = GasAmount>>(iter: I) -> Self {
        let total = iter.fold(Decimal256::zero(), |acc, x| acc + x.0);
        GasAmount(total)
    }
}

impl AddAssign for GasAmount {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl FromStr for GasAmount {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        s.parse().map(GasAmount).map_err(|e| e.into())
    }
}

impl Display for GasAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for GasAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "GasAmount{}", self.0)
    }
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ConfigTestnet {
    deployments: BTreeMap<String, DeploymentConfigTestnet>,
    overrides: BTreeMap<String, DeploymentConfigTestnet>,
    pub price_api: String,
    pub liquidity: LiquidityConfig,
    pub utilization: UtilizationConfig,
    pub trader: TraderConfig,
    /// QA wallet used for price updates
    pub qa_wallet: RawAddress,
    pub initial_prices: BTreeMap<MarketId, InitialPrice>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LiquidityConfig {
    /// Min and max per different markets
    pub markets: BTreeMap<MarketId, LiquidityBounds>,
    /// Lower bound of util ratio, at which point we would withdraw liquidity
    pub min_util_delta: Signed<Decimal256>,
    /// Upper bound of util ratio, at which point we would deposit liquidity
    pub max_util_delta: Signed<Decimal256>,
    /// When we deposit or withdraw, what utilization ratio do we target?
    pub target_util_delta: Signed<Decimal256>,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LiquidityTransactionConfig {
    /// Total number of blocks between which you need to check values
    pub number_of_blocks: u16,
    /// Percentage change of total liqudity below/above which we should alert
    pub liqudity_percentage: Decimal256,
    /// Percentage change of total deposits below/above which we should alert
    pub total_deposits_percentage: Decimal256,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct UtilizationConfig {
    /// Lower bound of util ratio, at which point we would open a position
    pub min_util_delta: Signed<Decimal256>,
    /// Upper bound of util ratio, at which point we would close a position
    pub max_util_delta: Signed<Decimal256>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TraderConfig {
    /// Upper bound of util ratio, at which point we always close a position
    pub max_util_delta: Signed<Decimal256>,
    /// Minimum borrow fee ratio. If below this, we always open positions.
    pub min_borrow_fee: Decimal256,
    /// Maximum borrow fee ratio. If above this, we always close a position.
    pub max_borrow_fee: Decimal256,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct LiquidityBounds {
    pub min: Collateral,
    pub max: Collateral,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DeploymentConfigTestnet {
    /// How many crank run wallets to set up
    #[serde(default)]
    pub crank: u32,
    /// How many ultracrank wallets to set up
    #[serde(default)]
    pub ultra_crank: u32,
    /// How many seconds behind we need to be before we kick in the ultracrank
    #[serde(default = "defaults::seconds_till_ultra")]
    pub seconds_till_ultra: u32,
    pub price: bool,
    pub wallet_manager_address: RawAddress,
    #[serde(default)]
    pub dev_settings: bool,
    #[serde(default)]
    pub trading_competition: bool,
    #[serde(default)]
    pub liquidity: bool,
    #[serde(default)]
    pub utilization: bool,
    #[serde(default)]
    pub balance: bool,
    #[serde(default)]
    pub traders: u32,
    pub default_market_ids: Vec<MarketId>,
    #[serde(default)]
    pub ignore_stale: bool,
    #[serde(default)]
    pub execs_per_price: Option<u32>,
    #[serde(default)]
    pub watcher: WatcherConfig,
    /// Minimum gas required in wallet managed by perps bots
    #[serde(default = "defaults::min_gas")]
    pub min_gas: GasAmount,
    /// Minimum gas required in very high gas wallet managed by perps bots
    #[serde(default = "defaults::min_gas_high_gas_wallet")]
    pub min_gas_high_gas_wallet: GasAmount,
    /// Minimum gas required in the faucet contract
    #[serde(default = "defaults::min_gas_in_faucet")]
    pub min_gas_in_faucet: GasAmount,
    /// Minimum gas required in the gas wallet
    #[serde(default = "defaults::min_gas_in_gas_wallet")]
    pub min_gas_in_gas_wallet: GasAmount,
    /// Number of seconds before a price update is forced
    #[serde(default = "defaults::max_price_age_secs")]
    pub max_price_age_secs: u32,
    /// Maximum the price can move before we push a price update, e.g. 0.01 means 1%.
    #[serde(default = "defaults::max_allowed_price_delta")]
    pub max_allowed_price_delta: Decimal256,
    /// Disable Pyth usage and instead use the QA wallet for price update
    #[serde(default)]
    pub qa_price_updates: bool,
}

impl ChainConfig {
    const PATH: &'static str = "packages/perps-exes/assets/config-chain.toml";

    pub fn load(network: PerpsNetwork) -> Result<Self> {
        Self::load_from(Self::PATH, network)
    }

    pub fn load_from_opt(config_file: Option<&Path>, network: PerpsNetwork) -> Result<Self> {
        match config_file {
            Some(config_file) => Self::load_from(config_file, network),
            None => Self::load(network),
        }
    }

    pub fn load_from(config_file: impl AsRef<Path>, network: PerpsNetwork) -> Result<Self> {
        load_toml::<_, BTreeMap<PerpsNetwork, Self>>(
            config_file,
            "LEVANA_CHAIN_CONFIG_",
            "chain config",
        )?
        .remove(&network)
        .with_context(|| format!("No chain config found for {network}"))
    }
}

impl ConfigTestnet {
    const PATH: &'static str = "packages/perps-exes/assets/config-testnet.toml";

    pub fn load() -> Result<Self> {
        Self::load_from(Self::PATH)
    }

    pub fn load_from_opt(config_file: Option<&Path>) -> Result<Self> {
        match config_file {
            Some(config_file) => Self::load_from(config_file),
            None => Self::load(),
        }
    }

    pub fn load_from(config_file: impl AsRef<Path>) -> Result<Self> {
        load_toml(config_file, "LEVANA_TESTNET_", "testnet config")
    }

    /// Provide the deployment name, such as osmodev, dragonqa, or seibeta
    pub fn get_deployment_info(&self, deployment: &str) -> Result<DeploymentInfo> {
        let (network, suffix) = parse_deployment(deployment)?;
        let wallet_phrase_name = suffix.to_ascii_uppercase();
        let partial_config = self.deployments.get(suffix).with_context(|| {
            format!(
                "No DeploymentInfo found for {}. Valid configs: {}",
                suffix,
                self.deployments
                    .keys()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        let partial = self
            .overrides
            .get(deployment)
            .unwrap_or(partial_config)
            .clone();
        Ok(DeploymentInfo {
            config: partial,
            network,
            wallet_phrase_name,
        })
    }
}

impl PriceConfig {
    const PATH: &'static str = "packages/perps-exes/assets/config-price.toml";

    pub fn load() -> Result<Self> {
        Self::load_from(Self::PATH)
    }

    pub fn load_from_opt(config_file: Option<&Path>) -> Result<Self> {
        match config_file {
            Some(config_file) => Self::load_from(config_file),
            None => Self::load(),
        }
    }

    pub fn load_from(config_file: impl AsRef<Path>) -> Result<Self> {
        load_toml(config_file, "LEVANA_PRICE_", "price config")
    }
}

pub struct DeploymentInfo {
    pub config: DeploymentConfigTestnet,
    pub network: PerpsNetwork,
    pub wallet_phrase_name: String,
}

/// Parse a deployment name (like osmobeta) into network and family (like osmosis-testnet and beta).
pub fn parse_deployment(deployment: &str) -> Result<(PerpsNetwork, &str)> {
    const NETWORKS: &[(PerpsNetwork, &str)] = &[
        (PerpsNetwork::Regular(CosmosNetwork::OsmosisTestnet), "osmo"),
        (PerpsNetwork::Regular(CosmosNetwork::SeiTestnet), "sei"),
        (
            PerpsNetwork::Regular(CosmosNetwork::InjectiveTestnet),
            "inj",
        ),
        (PerpsNetwork::DymensionTestnet, "dym"),
        (PerpsNetwork::Regular(CosmosNetwork::NeutronTestnet), "ntrn"),
        (PerpsNetwork::NibiruTestnet, "nibi"),
    ];
    for (network, prefix) in NETWORKS {
        if let Some(suffix) = deployment.strip_prefix(prefix) {
            return Ok((*network, suffix));
        }
    }
    Err(anyhow::anyhow!(
        "Could not parse deployment: {}",
        deployment
    ))
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct WatcherConfig {
    /// How many times to retry before giving up
    #[serde(default = "defaults::retries")]
    pub retries: usize,
    /// How many seconds to delay between retries
    #[serde(default = "defaults::delay_between_retries")]
    pub delay_between_retries: u32,
    #[serde(default = "defaults::balance")]
    pub balance: TaskConfig,
    #[serde(default = "defaults::gas_check")]
    pub gas_check: TaskConfig,
    #[serde(default = "defaults::liquidity")]
    pub liquidity: TaskConfig,
    #[serde(default = "defaults::trader")]
    pub trader: TaskConfig,
    #[serde(default = "defaults::utilization")]
    pub utilization: TaskConfig,
    #[serde(default = "defaults::track_balance")]
    pub track_balance: TaskConfig,
    #[serde(default = "defaults::crank_watch")]
    pub crank_watch: TaskConfig,
    #[serde(default = "defaults::crank_run")]
    pub crank_run: TaskConfig,
    #[serde(default = "defaults::get_factory")]
    pub get_factory: TaskConfig,
    #[serde(default = "defaults::price")]
    pub price: TaskConfig,
    #[serde(default = "defaults::stale")]
    pub stale: TaskConfig,
    #[serde(default = "defaults::stats")]
    pub stats: TaskConfig,
    #[serde(default = "defaults::stats_alert")]
    pub stats_alert: TaskConfig,
    #[serde(default = "defaults::ultra_crank")]
    pub ultra_crank: TaskConfig,
    #[serde(default = "defaults::liquidity_transaction_alert")]
    pub liquidity_transaction: TaskConfig,
    #[serde(default = "defaults::rpc_health")]
    pub rpc_health: TaskConfig,
    #[serde(default = "defaults::congestion")]
    pub congestion: TaskConfig,
    #[serde(default = "defaults::high_gas")]
    pub high_gas: TaskConfig,
    #[serde(default = "defaults::block_lag")]
    pub block_lag: TaskConfig,
    #[serde(default = "defaults::counter_trade_bot")]
    pub counter_trade_bot: TaskConfig,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            retries: defaults::retries(),
            delay_between_retries: defaults::delay_between_retries(),
            balance: TaskConfig {
                delay: Delay::Constant(20),
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
            gas_check: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
            liquidity: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
            trader: TaskConfig {
                delay: Delay::Random {
                    low: 120,
                    high: 1200,
                },
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
            utilization: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: Some(120),
                retries: None,
                delay_between_retries: None,
            },
            track_balance: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: Some(60),
                retries: None,
                delay_between_retries: None,
            },
            crank_watch: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: Some(60),
                retries: None,
                delay_between_retries: None,
            },
            crank_run: TaskConfig {
                // We block internally within the crank run service
                delay: Delay::NoDelay,
                out_of_date: None,
                retries: None,
                delay_between_retries: None,
            },
            get_factory: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: Some(180),
                retries: Some(5),
                delay_between_retries: Some(30),
            },
            price: TaskConfig {
                delay: Delay::NewBlock,
                out_of_date: Some(30),
                // Intentionally using different defaults to make sure price
                // updates come through quickly. We increase our retries to
                // compensate for the shorter delay.
                retries: Some(20),
                delay_between_retries: Some(1),
            },
            stale: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: Some(180),
                retries: Some(5),
                delay_between_retries: Some(20),
            },
            stats: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
            stats_alert: TaskConfig {
                delay: Delay::Constant(30),
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
            ultra_crank: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
            liquidity_transaction: TaskConfig {
                delay: Delay::Constant(120),
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
            rpc_health: TaskConfig {
                delay: Delay::Constant(300),
                out_of_date: Some(500),
                retries: None,
                delay_between_retries: None,
            },
            congestion: TaskConfig {
                // OK to be fast on this, we use cached data
                delay: Delay::Constant(2),
                out_of_date: Some(2),
                retries: None,
                delay_between_retries: None,
            },
            high_gas: TaskConfig {
                // We block internally within this service
                // and use a channel to signal when it should be woken up
                delay: Delay::NoDelay,
                out_of_date: None,
                retries: None,
                delay_between_retries: None,
            },
            block_lag: TaskConfig {
                delay: Delay::Constant(20),
                out_of_date: Some(20),
                retries: None,
                delay_between_retries: None,
            },
            counter_trade_bot: TaskConfig {
                delay: Delay::Constant(60),
                out_of_date: Some(180),
                retries: None,
                delay_between_retries: None,
            },
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TaskConfig {
    /// Seconds to delay between runs
    pub delay: Delay,
    /// How many seconds before we should consider the result out of date
    ///
    /// This does not include the delay time
    pub out_of_date: Option<u32>,
    /// How many times to retry before giving up, overriding the general watcher
    /// config
    pub retries: Option<usize>,
    /// How many seconds to delay between retries, overriding the general
    /// watcher config
    pub delay_between_retries: Option<u32>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Delay {
    NoDelay,
    Constant(u64),
    NewBlock,
    Random { low: u64, high: u64 },
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MarketConfigUpdates {
    pub markets: BTreeMap<MarketId, ConfigUpdateAndBorrowFee>,
    pub crank_fees: BTreeMap<PerpsNetwork, CrankFeeConfig>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CrankFeeConfig {
    pub charged: Usd,
    pub surcharge: Usd,
    pub reward: Usd,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigUpdateAndBorrowFee {
    pub config: ConfigUpdate,
    pub initial_borrow_fee_rate: Decimal256,
}

impl MarketConfigUpdates {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let config: Self = load_toml(path, "LEVANA_MARKETS_", "markets config")?;

        config.validate().with_context(|| {
            format!(
                "Unable to parse MarketConfigUpdates from {}",
                path.display()
            )
        })?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        enum Mismatch<'a> {
            CarryLeverage {
                market: &'a MarketId,
                configured: Decimal256,
                expected: Decimal256,
            },
            DnfCap {
                market: &'a MarketId,
                configured: NonZero<Decimal256>,
                expected: NonZero<Decimal256>,
            },
        }
        let mut mismatches: Vec<Mismatch> = vec![];

        let default_max_leverage = ConfigDefaults::max_leverage();
        let default_carry_leverage = ConfigDefaults::carry_leverage();
        let default_dnf_cap = ConfigDefaults::delta_neutrality_fee_cap();
        let seven: Decimal256 = "7".parse()?;
        let two: Decimal256 = "2".parse()?;
        for (market_id, update) in &self.markets {
            let max_leverage = update
                .config
                .max_leverage
                .unwrap_or(default_max_leverage)
                .abs_unsigned();
            let expected_carry_leverage = seven.min(max_leverage / two);
            let configured_carry_leverage = update
                .config
                .carry_leverage
                .unwrap_or(default_carry_leverage);

            if expected_carry_leverage != configured_carry_leverage {
                mismatches.push(Mismatch::CarryLeverage {
                    market: market_id,
                    configured: configured_carry_leverage,
                    expected: expected_carry_leverage,
                })
            }

            let expected_dnf_cap = match max_leverage.to_string().parse::<u32>()? {
                4 => "0.03",
                6 => "0.025",
                10 => "0.015",
                30 => "0.005",
                50 => "0.0002",
                max_leverage => anyhow::bail!("Unexpected max leverage value: {max_leverage}"),
            }
            .parse()?;
            let dnf_cap = update
                .config
                .delta_neutrality_fee_cap
                .unwrap_or(default_dnf_cap);
            // For now we're ignoring caps which are too low
            if dnf_cap > expected_dnf_cap {
                mismatches.push(Mismatch::DnfCap {
                    market: market_id,
                    configured: dnf_cap,
                    expected: expected_dnf_cap,
                })
            }
        }

        if mismatches.is_empty() {
            Ok(())
        } else {
            let mut msg = "Unexpected config values provided:\n".to_owned();
            for mismatch in mismatches {
                match mismatch {
                    Mismatch::CarryLeverage {
                        market,
                        configured,
                        expected,
                    } => {
                        writeln!(
                            &mut msg,
                            "{market} carry leverage: Expected: {expected}. Found: {configured}"
                        )?;
                    }
                    Mismatch::DnfCap {
                        market,
                        configured,
                        expected,
                    } => {
                        writeln!(
                            &mut msg,
                            "{market} DNF cap: Expected: {expected}. Found: {configured}"
                        )?;
                    }
                }
            }
            Err(anyhow::anyhow!("{msg}"))
        }
    }
}

/// Stores mainnet factory contracts
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MainnetFactories {
    pub factories: Vec<MainnetFactory>,
}

impl MainnetFactories {
    pub fn get_by_address(&self, address: Address) -> Option<&MainnetFactory> {
        self.factories.iter().find(|f| f.address == address)
    }

    pub fn get_by_ident(&self, ident: &str) -> Option<&MainnetFactory> {
        self.factories
            .iter()
            .find(|f| f.ident.as_deref() == Some(ident))
    }

    /// Gets by either address or ident
    pub fn get(&self, factory: &str) -> Result<&MainnetFactory> {
        match factory.parse().ok() {
            Some(addr) => self.get_by_address(addr),
            None => self.get_by_ident(factory),
        }
        .with_context(|| format!("Unknown factory: {factory}"))
    }
}

/// An instantiated factory on mainnet.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MainnetFactory {
    pub address: Address,
    pub network: PerpsNetwork,
    pub label: String,
    pub instantiate_code_id: u64,
    pub instantiate_at: DateTime<Utc>,
    pub gitrev: String,
    pub hash: String,
    /// A user-friendly identifier
    pub ident: Option<String>,
    /// Manually set flag to indicate that this factory should be included in any full mainnet stats.
    #[serde(default)]
    pub canonical: bool,
}

impl MainnetFactories {
    const PATH: &'static str = "packages/perps-exes/assets/mainnet-factories.toml";

    pub fn load() -> Result<Self> {
        load_toml(Self::PATH, "LEVANA_MAINNET_FACTORIES_", "mainnet factories")
    }

    pub fn save(&self) -> Result<()> {
        save_toml(Self::PATH, self)
    }
}

pub fn load_toml<P, T>(path: P, env_prefix: &str, config_desc: &str) -> Result<T>
where
    P: AsRef<Path>,
    T: serde::de::DeserializeOwned + std::fmt::Debug,
{
    let path = path.as_ref();
    let config = Figment::new()
        .merge(Toml::file(path))
        .merge(Env::prefixed(env_prefix))
        .extract()
        .with_context(|| format!("Unable to load {config_desc} from {}", path.display()))?;
    tracing::debug!("Loaded {config_desc}: {config:#?}");
    Ok(config)
}

pub fn save_toml<P, T>(path: P, value: &T) -> Result<()>
where
    P: AsRef<Path>,
    T: serde::Serialize,
{
    let path = path.as_ref();
    (|| fs_err::write(path, toml::to_string_pretty(value)?).map_err(anyhow::Error::from))()
        .with_context(|| format!("Unable to save TOML file {}", path.display()))
}
