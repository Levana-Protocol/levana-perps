#[derive(clap::Parser, Debug)]
pub(crate) struct Opt {
    /// Turn on verbose logging
    #[clap(long, short)]
    verbose: bool,
    /// Name of the factory where get open positions information
    #[clap(long, default_value = "osmosis1", env = "LEVANA_TRADERS_FACTORY")]
    pub(crate) factory: String,
    /// Slack webhook to send alert notification
    #[arg(long, env = "LEVANA_TRADERS_SLACK_WEBHOOK")]
    pub(crate) slack_webhook: reqwest::Url,
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
