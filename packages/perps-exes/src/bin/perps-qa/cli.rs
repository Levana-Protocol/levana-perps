use cosmos::{Address, SeedPhrase};
use perps_exes::{build_version, PerpsNetwork, UpdatePositionCollateralImpact};
use perpswap::{contracts::market::position::PositionId, prelude::*};
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Layer};

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
    /// Print balances
    PrintBalances {},
    /// Total positions opened in this contract
    TotalPosition {},
    /// All open positions that belong to your wallet
    AllOpenPositions {},
    /// All closed positions that belong to your wallet
    AllClosePositions {},
    /// Fund the market
    DepositLiquidity {
        /// Optional Collateral amount.
        #[clap(long, default_value = "1000000000")]
        fund: NonZero<Collateral>,
    },
    /// Open Position
    OpenPosition {
        /// Collateral amount in ATOM.
        #[clap(long)]
        collateral: NonZero<Collateral>,
        /// Leverage
        #[clap(long, allow_hyphen_values = true)]
        leverage: LeverageToBase,
        /// Max gains percentage
        #[clap(long, allow_hyphen_values = true)]
        max_gains: MaxGainsInQuote,
        /// Current USD Price
        #[clap(long)]
        current_price: Option<PriceBaseInQuote>,
        /// Max slippage percentage
        #[clap(long)]
        max_slippage: Option<Number>,
        #[clap(long)]
        short: bool,
    },
    /// Update Collateral
    UpdateCollateral {
        #[clap(long)]
        position_id: PositionId,
        /// New Collateral amount.
        #[clap(long)]
        collateral: Collateral,
        /// Impact
        #[clap(long)]
        impact: UpdatePositionCollateralImpact,
        /// Current USD Price
        #[clap(long, requires = "max_slippage")]
        current_price: Option<PriceBaseInQuote>,
        /// Max slippage percentage
        #[clap(long, requires = "current_price")]
        max_slippage: Option<Number>,
    },
    /// Close Position
    ClosePosition {
        #[clap(long)]
        position_id: PositionId,
    },
    /// Fetch latest price
    FetchPrice {},
    /// Set Latest price
    SetPrice {
        /// Current USD Price
        #[clap(long)]
        price: PriceBaseInQuote,
        /// Price of collateral assert in terms of USD
        #[clap(long)]
        price_usd: PriceCollateralInUsd,
    },
    /// Crank
    Crank {},
    /// Position Details
    PositionDetail {
        #[clap(long)]
        position_id: PositionId,
    },
    /// Tap Faucet
    TapFaucet {},
    /// Update Max Gains
    UpdateMaxGains {
        #[clap(long)]
        position_id: PositionId,
        /// Max gains percentage
        #[clap(long, allow_hyphen_values = true)]
        max_gains: MaxGainsInQuote,
    },
    /// Update Leverage
    UpdateLeverage {
        #[clap(long)]
        position_id: PositionId,
        /// Leverage
        #[clap(long, allow_hyphen_values = true)]
        leverage: LeverageToBase,
        /// Current USD Price
        #[clap(long, requires = "max_slippage")]
        current_price: Option<PriceBaseInQuote>,
        /// Max slippage percentage
        #[clap(long, requires = "current_price")]
        max_slippage: Option<Number>,
    },
    /// Print various stats
    Stats {},
    /// Print the config file
    GetConfig {},
    /// Generate a CSV file with historical max position sizes
    CappingReport {
        #[clap(flatten)]
        inner: crate::capping::Opt,
    },
    /// Generate a CSV file with historical wallet balances
    WalletReport {
        #[clap(flatten)]
        inner: crate::wallet::Opt,
    },
}

#[derive(clap::Parser)]
pub(crate) struct Opt {
    /// Network to use, overrides the contract family setting
    #[clap(long, env = "COSMOS_NETWORK", global = true)]
    pub network: Option<PerpsNetwork>,
    /// Override gRPC endpoint
    #[clap(long, env = "COSMOS_GRPC", global = true)]
    pub cosmos_grpc: Option<String>,
    /// Which contract family to send messages to
    #[clap(
        long,
        env = "LEVANA_PERP_CONTRACT_FAMILY",
        global = true,
        default_value = "osmoqa"
    )]
    pub contract_family: String,
    /// Perp factory contract address, overrides the contract family setting
    #[clap(long, env = "LEVANA_PERP_FACTORY_CONTRACT_ADDRESS", global = true)]
    pub factory_contract_address: Option<Address>,
    /// Perp faucet contract address, overrides the contract family setting
    #[clap(long, env = "LEVANA_PERP_FAUCET_CONTRACT_ADDRESS", global = true)]
    pub faucet_contract_address: Option<Address>,
    /// Market we want to interact with
    #[clap(
        long,
        env = "LEVANA_PERP_MARKET_ID",
        global = true,
        default_value = "ATOM_USD"
    )]
    pub market_id: MarketId,
    /// Mnemonic phrase for the Wallet
    #[clap(long, env = "COSMOS_WALLET")]
    pub wallet: SeedPhrase,
    /// Turn on verbose logging
    #[clap(long, short, global = true)]
    verbose: bool,
}

impl Opt {
    pub(crate) fn init_logger(&self) -> anyhow::Result<()> {
        let env_filter = EnvFilter::from_default_env();

        let crate_name = env!("CARGO_CRATE_NAME");
        let env_filter = match std::env::var("RUST_LOG") {
            Ok(_) => env_filter,
            Err(_) => {
                if self.verbose {
                    env_filter
                        .add_directive("cosmos=debug".parse()?)
                        .add_directive(format!("{}=debug", crate_name).parse()?)
                } else {
                    env_filter.add_directive(format!("{}=info", crate_name).parse()?)
                }
            }
        };

        tracing_subscriber::registry()
            .with(
                fmt::Layer::default()
                    .log_internal_errors(true)
                    .and_then(env_filter),
            )
            .init();

        tracing::debug!("Debug message!");
        Ok(())
    }
}
