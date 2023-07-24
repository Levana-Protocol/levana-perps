use cosmos::{Address, CosmosNetwork, RawWallet};
use msg::{contracts::market::position::PositionId, prelude::*};
use perps_exes::{build_version, UpdatePositionCollateralImpact};

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
        price_usd: Option<PriceCollateralInUsd>,
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
}

#[derive(clap::Parser)]
pub(crate) struct Opt {
    /// Network to use, overrides the contract family setting
    #[clap(long, env = "COSMOS_NETWORK", global = true)]
    pub network: Option<CosmosNetwork>,
    /// Override gRPC endpoint
    #[clap(long, env = "COSMOS_GRPC", global = true)]
    pub cosmos_grpc: Option<String>,
    /// Which contract family to send messages to
    #[clap(
        long,
        env = "LEVANA_PERP_CONTRACT_FAMILY",
        global = true,
        default_value = "dragonqa"
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
    pub wallet: RawWallet,
    /// Turn on verbose logging
    #[clap(long, short, global = true)]
    verbose: bool,
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
