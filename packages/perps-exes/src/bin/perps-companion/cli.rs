use std::{borrow::Cow, net::SocketAddr};

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

    #[clap(subcommand)]
    pub(crate) pgopt: PGOpt,
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
