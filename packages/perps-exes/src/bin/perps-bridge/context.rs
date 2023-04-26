use clap::Parser;
use futures::{channel::mpsc::UnboundedSender, lock::Mutex};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::fs::File;
use tokio_tungstenite::tungstenite::Message;

pub struct Context {
    pub opts: Options,
    pub peer_map: Mutex<PeerMap>,
    pub log_file: Mutex<Option<File>>,
}

type Tx = UnboundedSender<Message>;
type PeerMap = HashMap<SocketAddr, Tx>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LogFlag {
    QueryMarket,
    ExecMarket,
    RefreshPrice,
    Crank,
    MintCollateral,
    MintAndDepositLp,
    TimeJumpSeconds,
}

impl LogFlag {
    pub fn default_vec() -> Vec<Self> {
        vec![
            Self::ExecMarket,
            Self::RefreshPrice,
            Self::Crank,
            Self::MintCollateral,
            Self::MintAndDepositLp,
            Self::TimeJumpSeconds,
            //Self::QueryMarket
        ]
    }
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Options {
    #[clap(long, env = "STRESS_TEST_BRIDGE_PORT", default_value_t = 31337)]
    pub port: u16,

    #[clap(long, default_value_t = true)]
    pub verbose: bool,

    #[clap(long, default_value_t = true)]
    pub log: bool,

    #[clap(long, value_enum, default_values_t = LogFlag::default_vec())]
    pub log_flags: Vec<LogFlag>,

    #[clap(long, default_value = "../../bridge.log")]
    pub log_file_path: PathBuf,
}

impl Context {
    pub async fn new() -> Arc<Self> {
        let opts = Options::parse();

        Arc::new(Context {
            opts,
            peer_map: Mutex::new(HashMap::new()),
            log_file: Mutex::new(None),
        })
    }

    pub fn listen_addr(&self) -> String {
        format!("127.0.0.1:{}", self.opts.port)
    }
}
