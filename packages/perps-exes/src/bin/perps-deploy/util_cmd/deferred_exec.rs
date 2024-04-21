use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use cosmos::{Address, Cosmos, HasCosmos};
use msg::contracts::market::deferred_execution::{
    DeferredExecId, DeferredExecStatus, GetDeferredExecResp,
};
use parking_lot::Mutex;
use perps_exes::{
    config::MainnetFactories,
    contracts::{Factory, MarketInfo},
    prelude::{MarketContract, MarketId},
};
use tokio::task::JoinSet;

#[derive(clap::Parser)]
pub(super) struct DeferredExecCsvOpt {
    /// Factory
    #[clap(long)]
    factory: String,
    /// Output CSV file
    #[clap(long)]
    csv: PathBuf,
}

impl DeferredExecCsvOpt {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Record {
    market: MarketId,
    owner: Address,
    exec_id: DeferredExecId,
    status: Status,
    executed_time: Option<DateTime<Utc>>,
    executed_block: Option<i64>,
    reason: Option<String>,
    rendered: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Success,
    Failure,
    Pending,
}

struct AllExecsInner {
    markets: HashMap<MarketId, MarketExecs>,
    csv: csv::Writer<std::fs::File>,
    blocks: HashMap<DateTime<Utc>, i64>,
}

#[derive(Clone)]
struct AllExecs {
    inner: Arc<Mutex<AllExecsInner>>,
}

impl AllExecs {
    fn load(path: &Path) -> Result<Self> {
        let mut records = Vec::<Record>::new();
        if !path.try_exists()? {
            println!("Path does not yet exist: {}", path.display());
        } else {
            println!("Loading existing values from {}", path.display());

            for record in csv::Reader::from_path(path)?.into_deserialize() {
                records.push(record?);
            }
        }

        let mut res = AllExecsInner {
            markets: HashMap::new(),
            csv: csv::Writer::from_path(path)?,
            blocks: HashMap::new(),
        };

        for record in records {
            res.add_record(record)?;
        }
        Ok(AllExecs {
            inner: Arc::new(Mutex::new(res)),
        })
    }

    fn needs_query(&self, market_id: &MarketId, exec_id: DeferredExecId) -> bool {
        match self.inner.lock().markets.get(market_id) {
            None => true,
            Some(market) => match market.execs.get(&exec_id) {
                None => true,
                Some(record) => match record.status {
                    Status::Success | Status::Failure => false,
                    Status::Pending => true,
                },
            },
        }
    }

    fn add_record(&self, record: Record) -> Result<()> {
        self.inner.lock().add_record(record)
    }

    async fn get_block(
        &self,
        cosmos: &Cosmos,
        executed_time: DateTime<Utc>,
    ) -> Result<Option<i64>> {
        {
            let guard = self.inner.lock();
            if let Some(block) = guard.blocks.get(&executed_time) {
                return Ok(Some(*block));
            }
        }

        const DO_EXPENSIVE_BLOCK_SEARCH: bool = false;

        if DO_EXPENSIVE_BLOCK_SEARCH {
            cosmos
                .first_block_after(executed_time, None)
                .await
                .map(Some)
                .map_err(|e| e.into())
        } else {
            Ok(None)
        }
    }
}

impl AllExecsInner {
    fn add_record(&mut self, record: Record) -> Result<()> {
        self.csv.serialize(&record)?;
        self.csv.flush()?;
        if let (Some(time), Some(height)) = (record.executed_time, record.executed_block) {
            self.blocks.insert(time, height);
        }
        self.markets
            .entry(record.market.clone())
            .or_default()
            .add_record(record);
        Ok(())
    }
}

#[derive(Default)]
struct MarketExecs {
    // If we cared more about performance, a Vec would make much more sense
    execs: HashMap<DeferredExecId, Record>,
}

impl MarketExecs {
    fn add_record(&mut self, record: Record) {
        self.execs.insert(record.exec_id, record);
    }
}

async fn go(
    opt: crate::cli::Opt,
    DeferredExecCsvOpt { factory, csv }: DeferredExecCsvOpt,
) -> Result<()> {
    let factories = MainnetFactories::load(None)?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let all_execs = AllExecs::load(&csv)?;

    let markets = factory.get_markets().await?;

    let mut set = JoinSet::new();
    for market in markets {
        set.spawn(go_market(all_execs.clone(), market));
    }

    while let Some(res) = set.join_next().await {
        match res.map_or_else(|e| Err(e.into()), |x| x) {
            Ok(()) => (),
            Err(e) => {
                set.abort_all();
                return Err(e);
            }
        }
    }

    Ok(())
}

async fn go_market(all_execs: AllExecs, market_info: MarketInfo) -> Result<()> {
    let market = MarketContract::new(market_info.market);
    let cosmos = market.get_cosmos();

    let mut exec_id = DeferredExecId::first();

    loop {
        if all_execs.needs_query(&market_info.market_id, exec_id) {
            match market.get_deferred_exec(exec_id).await? {
                GetDeferredExecResp::Found { item } => {
                    let rendered = serde_json::to_string(&item)?;
                    let (status, executed_time, reason) = match item.status {
                        DeferredExecStatus::Pending => (Status::Pending, None, None),
                        DeferredExecStatus::Success {
                            target: _,
                            executed,
                        } => (
                            Status::Success,
                            Some(executed.try_into_chrono_datetime()?),
                            None,
                        ),
                        DeferredExecStatus::Failure {
                            reason,
                            executed,
                            crank_price: _,
                        } => (
                            Status::Failure,
                            Some(executed.try_into_chrono_datetime()?),
                            Some(reason),
                        ),
                    };
                    let executed_block = match executed_time {
                        None => None,
                        Some(executed_time) => all_execs.get_block(cosmos, executed_time).await?,
                    };

                    let record = Record {
                        market: market_info.market_id.clone(),
                        exec_id,
                        rendered,
                        owner: item.owner.as_str().parse()?,
                        status,
                        reason,
                        executed_time,
                        executed_block,
                    };
                    all_execs.add_record(record)?;
                }
                GetDeferredExecResp::NotFound {} => break Ok(()),
            }
        }
        exec_id = exec_id.next();
    }
}
