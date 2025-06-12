use std::{borrow::Cow, net::SocketAddr};
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Layer};

#[derive(clap::Parser)]
pub(crate) struct Opt {
    #[clap(long, short)]
    verbose: bool,
    #[clap(
        long,
        default_value = "[::]:3000",
        env = "LEVANA_COMPANION_BIND",
        global = true
    )]
    pub(crate) bind: SocketAddr,

    /// Primary mainnet GRPC Override for Osmosis.
    #[clap(
        long,
        env = "LEVANA_COMPANION_OSMOSIS_MAINNET_PRIMARY_GRPC",
        default_value = "https://grpc.osmosis.zone"
    )]
    pub(crate) osmosis_mainnet_primary: String,

    /// Primary mainnet GRPC Override for Sei.
    #[clap(
        long,
        env = "LEVANA_COMPANION_SEI_MAINNET_PRIMARY_GRPC",
        default_value = "https://grpc.sei-apis.com"
    )]
    pub(crate) sei_mainnet_primary: String,

    /// Primary mainnet GRPC Override for Injective.
    #[clap(
        long,
        env = "LEVANA_COMPANION_INJECTIVE_MAINNET_PRIMARY_GRPC",
        default_value = "https://inj-priv-grpc.kingnodes.com"
    )]
    pub(crate) injective_mainnet_primary: String,

    /// Primary mainnet GRPC Override for Neutron.
    #[clap(
        long,
        env = "LEVANA_COMPANION_NEUTRON_MAINNET_PRIMARY_GRPC",
        default_value = "http://grpc-kralum.neutron-1.neutron.org"
    )]
    pub(crate) neutron_mainnet_primary: String,

    /// Fallback GRPC endpoints for Osmosis mainnet.
    #[clap(
        long,
        env = "LEVANA_COMPANION_OSMOSIS_MAINNET_FALLBACKS_GRPC",
        value_delimiter = ',',
        default_value = "https://osmo-priv-grpc.kingnodes.com"
    )]
    pub(crate) osmosis_mainnet_fallbacks: Vec<String>,

    /// Primary GRPC endpoints for Rujira testnet.
    #[clap(
        long,
        env = "LEVANA_COMPANION_RUJIRA_TESTNET_GRPC",
        value_delimiter = ',',
        default_value = "https://stagenet-grpc.ninerealms.com:443"
    )]
    pub(crate) rujira_testnet_primary: String,

    /// Primary GRPC endpoints for Rujira mainnet.
    #[clap(
        long,
        env = "LEVANA_COMPANION_RUJIRA_MAINNET_GRPC",
        value_delimiter = ',',
        default_value = "https://thornode-mainnet-grpc.bryanlabs.net:443"
    )]
    pub(crate) rujira_mainnet_primary: String,

    /// Fallback GRPC endpoints for Injective mainnet.
    #[clap(
        long,
        env = "LEVANA_COMPANION_INJECTIVE_MAINNET_FALLBACKS_GRPC",
        value_delimiter = ',',
        default_value = "https://sentry.chain.grpc.injective.network"
    )]
    pub(crate) injective_mainnet_fallbacks: Vec<String>,

    /// Fallback GRPC endpoints for Sei mainnet.
    #[clap(
        long,
        env = "LEVANA_COMPANION_SEI_MAINNET_FALLBACKS_GRPC",
        value_delimiter = ',',
        default_value = "https://sei-grpc.lavenderfive.com"
    )]
    pub(crate) sei_mainnet_fallbacks: Vec<String>,

    /// Fallback GRPC endpoints for Neutron mainnet.
    #[clap(
        long,
        env = "LEVANA_COMPANION_NEUTRON_MAINNET_FALLBACKS_GRPC",
        value_delimiter = ',',
        default_value = "http://c7f58ef9-1d78-4e15-a818-d02c8f50fc67.neutron-1.mesa-grpc.newmetric.xyz"
    )]
    pub(crate) neutron_mainnet_fallbacks: Vec<String>,

    /// Fallback GRPC endpoints for Rujira testnet.
    #[clap(
        long,
        env = "LEVANA_COMPANION_RUJIRA_TESTNET_FALLBACKS_GRPC",
        value_delimiter = ','
    )]
    pub(crate) rujira_testnet_fallbacks: Vec<String>,

    /// Fallback GRPC endpoints for Rujira mainnet.
    #[clap(
        long,
        env = "LEVANA_COMPANION_RUJIRA_MAINNET_FALLBACKS_GRPC",
        value_delimiter = ','
    )]
    pub(crate) rujira_mainnet_fallbacks: Vec<String>,

    /// Reqests timeout in seconds
    #[clap(long, env = "LEVANA_COMPANION_REQUEST_TIMEOUT", default_value_t = 5)]
    pub(crate) request_timeout_seconds: u64,
    /// Reqests timeout in seconds
    #[clap(
        long,
        env = "LEVANA_COMPANION_EXPORT_REQUEST_TIMEOUT",
        default_value_t = 10
    )]
    pub(crate) export_handler_timeout_seconds: u64,

    /// Body length limit in bytes. Default is 1MB (Same as Nginx)
    #[clap(long, env = "LEVANA_COMPANION_BODY_LIMIT", default_value_t = 1024000)]
    pub(crate) request_body_limit_bytes: usize,

    #[clap(subcommand)]
    pub(crate) pgopt: PGOpt,

    /// Require that the fonts needed by the SVG are present
    #[clap(long, env = "LEVANA_COMPANION_FONT_CHECK")]
    pub(crate) font_check: bool,

    /// Cache-bust query string parameter to force Twitter to reindex metadata
    #[clap(long, env = "LEVANA_COMPANION_CACHE_BUST", default_value_t = 1)]
    pub(crate) cache_bust: u32,
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
                        .add_directive("levana=debug".parse()?)
                        .add_directive(format!("{}=debug", crate_name).parse()?)
                } else {
                    env_filter
                        .add_directive(format!("{}=info", crate_name).parse()?)
                        .add_directive("tower_http=info".parse()?)
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

#[derive(clap::Parser, Clone)]
pub(crate) enum PGOpt {
    Uri {
        #[clap(long, env = "LEVANA_COMPANION_POSTGRES_URI")]
        postgres_uri: String,
    },
    Individual {
        #[clap(long, env = "PGHOST")]
        host: String,
        #[clap(long, env = "PGPORT")]
        port: String,
        #[clap(long, env = "PGDATABASE")]
        database: String,
        #[clap(long, env = "PGUSER")]
        user: String,
        #[clap(long, env = "PGPASSWORD")]
        password: String,
    },
}

impl PGOpt {
    pub(crate) fn uri(&self) -> Cow<str> {
        match self {
            PGOpt::Uri { postgres_uri } => postgres_uri.into(),
            PGOpt::Individual {
                host,
                port,
                database,
                user,
                password,
            } => format!("postgresql://{user}:{password}@{host}:{port}/{database}").into(),
        }
    }
}
