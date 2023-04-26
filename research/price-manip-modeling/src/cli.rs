use std::path::PathBuf;

#[derive(clap::Parser)]
pub(crate) struct Cmd {
    #[clap(flatten)]
    opt: Opt,
    #[clap(subcommand)]
    pub(crate) subcommand: Subcommand,
}

#[derive(clap::Parser)]
struct Opt {
    /// Turn on verbose logging
    #[clap(long, global = true)]
    verbose: bool,
}

#[derive(clap::Parser)]
pub(crate) enum Subcommand {
    Simulate {
        #[clap(long, default_value = "price-manip-results.csv")]
        results: PathBuf,
    },
    Single {},
}

impl Cmd {
    pub(crate) fn init_logger(&self) {
        let env = env_logger::Env::default().default_filter_or(if self.opt.verbose {
            format!("{}=debug,info", env!("CARGO_CRATE_NAME"))
        } else {
            "info".to_owned()
        });
        env_logger::Builder::from_env(env).init();
    }
}
