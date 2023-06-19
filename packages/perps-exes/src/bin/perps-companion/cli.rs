use std::net::SocketAddr;

#[derive(clap::Parser)]
pub(crate) struct Opt {
    #[clap(long, short)]
    verbose: bool,
    #[clap(
        long,
        default_value = "0.0.0.0:3000",
        env = "LEVANA_COMPANION_BIND",
        global = true
    )]
    pub(crate) bind: SocketAddr,
    #[clap(long, env = "LEVANA_COMPANION_POSTGRES_URI")]
    pub(crate) postgres_uri: String,
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
