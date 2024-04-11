use std::borrow::Cow;
use std::fmt::{Display, Write};
use std::pin::Pin;
use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use axum::http::HeaderValue;
use axum::response::IntoResponse;
use axum::{async_trait, Json};
use chrono::{DateTime, Duration, Utc};

use perps_exes::build_version;
use perps_exes::config::{TaskConfig, WatcherConfig};
use rand::Rng;

use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::task::JoinSet;

use crate::app::factory::FrontendInfoTestnet;
use crate::app::AppBuilder;
use crate::app::{factory::FactoryInfo, App};
use crate::endpoints::start_rest_api;
use crate::util::markets::Market;

/// Different kinds of tasks that we can watch
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub(crate) enum TaskLabel {
    GetFactory,
    Stale,
    CrankRun { index: usize },
    Price,
    TrackBalance,
    Stats,
    StatsAlert,
    GasCheck,
    Liquidity,
    Utilization,
    Balance,
    UltraCrank { index: usize },
    Trader { index: u32 },
    LiqudityTransactionAlert,
    TotalDepositAlert,
    RpcHealth,
    Congestion,
    HighGas,
    BlockLag,
}

impl TaskLabel {
    pub(crate) fn from_slug(s: &str) -> Option<TaskLabel> {
        match s {
            "get-factory" => Some(TaskLabel::GetFactory),
            "stale" => Some(TaskLabel::Stale),
            "price" => Some(TaskLabel::Price),
            "track-balance" => Some(TaskLabel::TrackBalance),
            "stats" => Some(TaskLabel::Stats),
            "stats-alert" => Some(TaskLabel::StatsAlert),
            "gas-check" => Some(TaskLabel::GasCheck),
            "liquidity" => Some(TaskLabel::Liquidity),
            "utilization" => Some(TaskLabel::Utilization),
            "balance" => Some(TaskLabel::Balance),
            "liquidity-transaction-alert" => Some(TaskLabel::LiqudityTransactionAlert),
            "total-deposit-alert" => Some(TaskLabel::TotalDepositAlert),
            "rpc-health" => Some(TaskLabel::RpcHealth),
            "congestion" => Some(TaskLabel::Congestion),
            "high-gas" => Some(TaskLabel::HighGas),
            "block-lag" => Some(TaskLabel::BlockLag),
            _ => {
                // Being lazy, skipping UltraCrank and Trader, they aren't needed
                let index = s.strip_prefix("crank-run-")?;
                let index = index.parse().ok()?;
                Some(TaskLabel::CrankRun { index })
            }
        }
    }

    fn show_output(&self) -> bool {
        match self {
            TaskLabel::GetFactory => false,
            TaskLabel::Stale => false,
            TaskLabel::CrankRun { index: _ } => true,
            TaskLabel::Price => true,
            TaskLabel::TrackBalance => false,
            TaskLabel::Stats => false,
            TaskLabel::StatsAlert => false,
            TaskLabel::GasCheck => false,
            TaskLabel::Liquidity => false,
            TaskLabel::Utilization => false,
            TaskLabel::Balance => false,
            TaskLabel::UltraCrank { index: _ } => false,
            TaskLabel::Trader { index: _ } => false,
            TaskLabel::LiqudityTransactionAlert => false,
            TaskLabel::TotalDepositAlert => false,
            TaskLabel::RpcHealth => false,
            TaskLabel::Congestion => false,
            TaskLabel::HighGas => true,
            TaskLabel::BlockLag => false,
        }
    }
}

impl Display for TaskLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TaskLabel::Trader { index } => write!(f, "Trader #{index}"),
            TaskLabel::UltraCrank { index } => write!(f, "Ultra crank #{index}"),
            TaskLabel::CrankRun { index } => write!(f, "Crank run #{index}"),
            x => write!(f, "{x:?}"),
        }
    }
}

impl serde::Serialize for TaskLabel {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct ToSpawn {
    future: Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>,
    label: TaskLabel,
}

#[derive(Default)]
pub(crate) struct Watcher {
    to_spawn: Vec<ToSpawn>,
    set: JoinSet<Result<()>>,
    statuses: StatusMap,
}

pub(crate) type StatusMap = HashMap<TaskLabel, Arc<RwLock<TaskStatus>>>;

#[derive(Default, Clone)]
pub(crate) struct TaskStatuses {
    statuses: Arc<StatusMap>,
}

#[derive(Clone, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskStatus {
    last_result: TaskResult,
    last_retry_error: Option<TaskError>,
    current_run_started: Option<DateTime<Utc>>,
    /// Is the last_result out of date ?
    #[serde(skip)]
    out_of_date: Option<Duration>,
    /// Should we expire the status of last result ?
    #[serde(skip)]
    expire_last_result: Option<Duration>,
    counts: TaskCounts,
}

#[derive(Clone, Copy, Default, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskCounts {
    pub(crate) successes: usize,
    pub(crate) retries: usize,
    pub(crate) errors: usize,
}
impl TaskCounts {
    fn total(&self) -> usize {
        self.successes + self.retries + self.errors
    }
}

#[derive(Clone, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskResult {
    pub(crate) value: Arc<TaskResultValue>,
    pub(crate) updated: DateTime<Utc>,
}

#[derive(Clone, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TaskResultValue {
    Ok(Cow<'static, str>),
    Err(String),
    NotYetRun,
}

const NOT_YET_RUN_MESSAGE: &str = "Task has not yet completed a single run";

impl TaskResultValue {
    fn as_str(&self) -> &str {
        match self {
            TaskResultValue::Ok(s) => s,
            TaskResultValue::Err(s) => s,
            TaskResultValue::NotYetRun => NOT_YET_RUN_MESSAGE,
        }
    }
}

#[derive(Clone, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskError {
    pub(crate) value: Arc<String>,
    pub(crate) updated: DateTime<Utc>,
}

enum OutOfDateType {
    Not,
    Slightly,
    Very,
}

impl TaskStatus {
    fn is_expired(&self) -> bool {
        if let Some(expiry_duration) = self.expire_last_result {
            let last_run = self.last_result.updated;
            let now = Utc::now();
            last_run + expiry_duration <= now
        } else {
            false
        }
    }

    fn is_out_of_date(&self) -> OutOfDateType {
        match self.current_run_started {
            Some(started) => match self.out_of_date {
                Some(out_of_date) => {
                    let now = Utc::now();
                    if started + Duration::seconds(300) <= now {
                        OutOfDateType::Very
                    } else if started + out_of_date <= now {
                        OutOfDateType::Slightly
                    } else {
                        OutOfDateType::Not
                    }
                }
                None => OutOfDateType::Not,
            },
            None => OutOfDateType::Not,
        }
    }
}

impl TaskLabel {
    fn task_config_for(&self, config: &WatcherConfig) -> TaskConfig {
        match self {
            TaskLabel::Balance => config.balance,
            TaskLabel::Stale => config.stale,
            TaskLabel::GasCheck => config.gas_check,
            TaskLabel::UltraCrank { index: _ } => config.ultra_crank,
            TaskLabel::Liquidity => config.liquidity,
            TaskLabel::Trader { index: _ } => config.trader,
            TaskLabel::Utilization => config.utilization,
            TaskLabel::TrackBalance => config.track_balance,
            TaskLabel::CrankRun { index: _ } => config.crank_run,
            TaskLabel::GetFactory => config.get_factory,
            TaskLabel::Price => config.price,
            TaskLabel::Stats => config.stats,
            TaskLabel::StatsAlert => config.stats_alert,
            TaskLabel::LiqudityTransactionAlert => config.liquidity_transaction,
            TaskLabel::TotalDepositAlert => config.liquidity_transaction,
            TaskLabel::RpcHealth => config.rpc_health,
            TaskLabel::Congestion => config.congestion,
            TaskLabel::HighGas => config.high_gas,
            TaskLabel::BlockLag => config.block_lag,
        }
    }

    fn triggers_alert(&self, selected_label: Option<TaskLabel>) -> bool {
        // If we loaded up a specific status page, always treat it as an alert.
        if selected_label.as_ref() == Some(self) {
            return true;
        }
        match self {
            TaskLabel::GetFactory => true,
            TaskLabel::CrankRun { index: _ } => true,
            TaskLabel::Price => true,
            TaskLabel::TrackBalance => false,
            TaskLabel::GasCheck => false,
            TaskLabel::UltraCrank { index: _ } => false,
            TaskLabel::Liquidity => false,
            TaskLabel::Utilization => false,
            TaskLabel::Balance => false,
            TaskLabel::Trader { index: _ } => false,
            TaskLabel::Stale => true,
            TaskLabel::Stats => true,
            TaskLabel::StatsAlert => false,
            TaskLabel::LiqudityTransactionAlert => false,
            TaskLabel::TotalDepositAlert => false,
            TaskLabel::RpcHealth => false,
            TaskLabel::Congestion => false,
            TaskLabel::HighGas => true,
            TaskLabel::BlockLag => true,
        }
    }

    fn ident(self) -> Cow<'static, str> {
        match self {
            TaskLabel::GetFactory => "get-factory".into(),
            TaskLabel::CrankRun { index } => format!("crank-run-{index}").into(),
            TaskLabel::Price => "price".into(),
            TaskLabel::TrackBalance => "track-balance".into(),
            TaskLabel::GasCheck => "gas-check".into(),
            TaskLabel::Liquidity => "liquidity".into(),
            TaskLabel::Utilization => "utilization".into(),
            TaskLabel::Balance => "balance".into(),
            TaskLabel::Trader { index } => format!("trader-{index}").into(),
            TaskLabel::Stale => "stale".into(),
            TaskLabel::Stats => "stats".into(),
            TaskLabel::UltraCrank { index } => format!("ultra-crank-{index}").into(),
            TaskLabel::StatsAlert => "stats-alert".into(),
            TaskLabel::LiqudityTransactionAlert => "liquidity-transaction-alert".into(),
            TaskLabel::TotalDepositAlert => "total-deposit-alert".into(),
            TaskLabel::RpcHealth => "rpc-health".into(),
            TaskLabel::Congestion => "congestion".into(),
            TaskLabel::HighGas => "high-gas".into(),
            TaskLabel::BlockLag => "block-lag".into(),
        }
    }
}

impl Watcher {
    pub(crate) async fn wait(mut self, app: Arc<App>, listener: TcpListener) -> Result<()> {
        self.set.spawn(start_rest_api(
            app,
            TaskStatuses {
                statuses: Arc::new(self.statuses),
            },
            listener,
        ));
        self.set.spawn(async move {
            loop {
                let now = Utc::now();
                println!("Heartbeat check: {now}");
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            }
        });
        for ToSpawn { future, label } in self.to_spawn {
            self.set.spawn(async move {
                future
                    .await
                    .with_context(|| format!("Failure while running: {label}"))
            });
        }

        while let Some(res) = self.set.join_next().await {
            if let Err(e) = res.map_err(|e| e.into()).and_then(|res| res) {
                self.set.abort_all();
                return Err(e);
            }
        }

        Ok(())
    }
}

impl AppBuilder {
    /// Watch a background job that runs continuously, launched immediately
    pub(crate) fn watch_background<Fut>(&mut self, task: Fut)
    where
        Fut: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        self.watcher.set.spawn(task);
    }

    pub(crate) fn watch_periodic<T>(&mut self, label: TaskLabel, mut task: T) -> Result<()>
    where
        T: WatchedTask,
    {
        let config = label.task_config_for(&self.app.config.watcher);
        let out_of_date = config
            .out_of_date
            .map(|item| chrono::Duration::seconds(item.into()));
        let task_status = Arc::new(RwLock::new(TaskStatus {
            last_result: TaskResult {
                value: TaskResultValue::NotYetRun.into(),
                updated: Utc::now(),
            },
            last_retry_error: None,
            current_run_started: None,
            out_of_date,
            counts: Default::default(),
            expire_last_result: None,
        }));
        {
            let old = self.watcher.statuses.insert(label, task_status.clone());
            if old.is_some() {
                anyhow::bail!("Two periodic tasks with label {label:?}");
            }
        }
        let app = self.app.clone();
        let future = Box::pin(async move {
            let mut last_seen_block_height = 0;
            let mut retries = 0;
            loop {
                let old_counts = {
                    let mut guard = task_status.write().await;
                    let old = &*guard;
                    *guard = TaskStatus {
                        last_result: old.last_result.clone(),
                        last_retry_error: old.last_retry_error.clone(),
                        current_run_started: Some(Utc::now()),
                        out_of_date,
                        counts: old.counts,
                        expire_last_result: old.expire_last_result,
                    };
                    guard.counts
                };
                let res = task
                    .run_single_with_timeout(
                        app.clone(),
                        Heartbeat {
                            task_status: task_status.clone(),
                        },
                        out_of_date.is_some(),
                    )
                    .await;
                let res = match res {
                    Ok(x) => Ok(x),
                    Err(err) => {
                        if app.cosmos.is_chain_paused() {
                            Ok(WatchedTaskOutput {
                            skip_delay: false,
                            suppress: false,
                            message: format!("Ignoring an error because the chain appears to be paused (Osmosis epoch). Error was:\n{err:?}").into(),
                                expire_alert: None,
				error: false
                        })
                        } else {
                            Err(err)
                        }
                    }
                };
                match res {
                    Ok(WatchedTaskOutput {
                        skip_delay,
                        message,
                        suppress,
                        expire_alert,
                        error,
                    }) => {
                        if label.show_output() {
                            tracing::info!("{label}: Success! {message}");
                        } else {
                            tracing::debug!("{label}: Success! {message}");
                        }
                        {
                            let mut guard = task_status.write().await;
                            let old = &*guard;
                            let title = label.to_string();
                            if label.triggers_alert(None) {
                                match &*old.last_result.value {
                                    TaskResultValue::Ok(_) => {
                                        if error {
                                            // Was a success, but not a success now
                                            sentry::with_scope(
                                                |scope| scope.set_tag("part-name", title.clone()),
                                                || {
                                                    sentry::capture_message(
                                                        &format!("{title}: {message}"),
                                                        sentry::Level::Error,
                                                    )
                                                },
                                            );
                                        }
                                    }
                                    TaskResultValue::Err(err) => {
                                        sentry::with_scope(
                                            |scope| scope.set_tag("part-name", title.clone()),
                                            || {
                                                sentry::capture_message(
                                                    &format!("{title} Recovered: {err}"),
                                                    sentry::Level::Info,
                                                )
                                            },
                                        );
                                    }
                                    TaskResultValue::NotYetRun => {
                                        // Bot newly started
                                        sentry::with_scope(
                                            |scope| scope.set_tag("part-name", title.clone()),
                                            || {
                                                sentry::capture_message(
                                            &format!("{title}: Bot restarted. This piece of the bots is not currently broken"),
                                            sentry::Level::Info,
                                        )
                                            },
                                        );
                                    }
                                }
                            }
                            *guard = TaskStatus {
                                last_result: TaskResult {
                                    value: if suppress {
                                        guard.last_result.value.clone()
                                    } else if error {
                                        TaskResultValue::Err(message.into()).into()
                                    } else {
                                        TaskResultValue::Ok(message).into()
                                    },
                                    updated: Utc::now(),
                                },
                                last_retry_error: None,
                                current_run_started: None,
                                out_of_date,
                                counts: TaskCounts {
                                    successes: if error {
                                        old_counts.successes
                                    } else {
                                        old_counts.successes + 1
                                    },
                                    errors: if error {
                                        old_counts.errors + 1
                                    } else {
                                        old_counts.errors
                                    },
                                    ..old_counts
                                },
                                expire_last_result: expire_alert,
                            };
                        }
                        retries = 0;
                        if !skip_delay {
                            match config.delay {
                                perps_exes::config::Delay::NoDelay => (),
                                perps_exes::config::Delay::Constant(secs) => {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(secs))
                                        .await;
                                }
                                perps_exes::config::Delay::Random { low, high } => {
                                    let secs = rand::thread_rng().gen_range(low..=high);
                                    tokio::time::sleep(tokio::time::Duration::from_secs(secs))
                                        .await;
                                }
                                perps_exes::config::Delay::NewBlock => {
                                    if let Some(duration) = app.config.price_bot_delay {
                                        tokio::time::sleep(duration).await;
                                    }

                                    // Wait for a new block to appear
                                    loop {
                                        match app.cosmos.get_latest_block_info().await {
                                            Ok(latest) => {
                                                if last_seen_block_height < latest.height {
                                                    last_seen_block_height = latest.height;
                                                    break;
                                                }
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "Unable to query latest block info: {e}"
                                                );
                                            }
                                        }
                                        tokio::time::sleep(tokio::time::Duration::from_millis(200))
                                            .await;
                                    }
                                }
                            };
                        }
                    }
                    Err(err) => {
                        if label.show_output() {
                            tracing::warn!("{label}: Error: {err:?}");
                        } else {
                            tracing::debug!("{label}: Error: {err:?}");
                        }
                        retries += 1;
                        let max_retries = config.retries.unwrap_or(app.config.watcher.retries);
                        // We want to get to first failure quickly so we don't
                        // have a misleading success status page. So if this
                        // failed and there are no prior attempts, don't retry.
                        if retries >= max_retries || task_status.read().await.counts.total() == 0 {
                            retries = 0;
                            let mut guard = task_status.write().await;
                            let old = &*guard;
                            let title = label.to_string();
                            let new_error_message = format!("{err:?}");

                            // Sentry/OpsGenie: only send alerts for labels that require it
                            if label.triggers_alert(None) {
                                match &*old.last_result.value {
                                    // The same error is happening as before
                                    TaskResultValue::Err(e) if e == &new_error_message => (),

                                    // Previous state is a different error. Update Sentry.
                                    TaskResultValue::Err(e) => {
                                        // New error occurs.
                                        sentry::with_scope(
                                            |scope| scope.set_tag("part-name", title.clone()),
                                            || {
                                                sentry::capture_message(
                                                    &format!("{title}: {new_error_message}"),
                                                    sentry::Level::Error,
                                                )
                                            },
                                        );
                                        sentry::with_scope(
                                            |scope| scope.set_tag("part-name", title.clone()),
                                            || {
                                                sentry::capture_message(
                                                    &format!("{title} May Recover: {e:?}"),
                                                    sentry::Level::Info,
                                                )
                                            },
                                        );
                                    }
                                    // Previous state is either unknown (NotYetRun), Ok Update Sentry.
                                    _ => {
                                        sentry::with_scope(
                                            |scope| scope.set_tag("part-name", title.clone()),
                                            || {
                                                sentry::capture_message(
                                                    &format!("{title}: {new_error_message}"),
                                                    sentry::Level::Error,
                                                )
                                            },
                                        );
                                    }
                                }
                            }
                            *guard = TaskStatus {
                                last_result: TaskResult {
                                    value: TaskResultValue::Err(new_error_message).into(),
                                    updated: Utc::now(),
                                },
                                last_retry_error: None,
                                current_run_started: None,
                                out_of_date,
                                counts: TaskCounts {
                                    errors: old_counts.errors + 1,
                                    ..old_counts
                                },
                                expire_last_result: None,
                            };
                        } else {
                            {
                                let mut guard = task_status.write().await;
                                let old = &*guard;
                                *guard = TaskStatus {
                                    last_result: old.last_result.clone(),
                                    last_retry_error: Some(TaskError {
                                        value: format!("{err:?}").into(),
                                        updated: Utc::now(),
                                    }),
                                    current_run_started: None,
                                    out_of_date,
                                    counts: TaskCounts {
                                        retries: old_counts.retries + 1,
                                        ..old_counts
                                    },
                                    expire_last_result: None,
                                };
                            }
                        }

                        tokio::time::sleep(tokio::time::Duration::from_secs(
                            config
                                .delay_between_retries
                                .unwrap_or(app.config.watcher.delay_between_retries)
                                .into(),
                        ))
                        .await;
                    }
                }
            }
        });
        self.watcher.to_spawn.push(ToSpawn { future, label });
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct WatchedTaskOutput {
    /// Should we skip delay between tasks ? If yes, then we dont
    /// sleep once the task gets completed.
    skip_delay: bool,
    /// Should we supress the output ? If we supress, the new output
    /// won't be reflected. The last_result value will be used instead.
    suppress: bool,
    message: Cow<'static, str>,
    /// Controls the stickiness of this message. After how long should
    /// we treat this as a non alert ?
    expire_alert: Option<Duration>,
    /// Is the message an error ?
    error: bool,
}

impl WatchedTaskOutput {
    pub(crate) fn new(message: impl Into<Cow<'static, str>>) -> Self {
        WatchedTaskOutput {
            skip_delay: false,
            suppress: false,
            message: message.into(),
            expire_alert: None,
            error: false,
        }
    }

    pub(crate) fn set_expiry(mut self, expire_duration: Duration) -> Self {
        self.expire_alert = Some(expire_duration);
        self
    }

    pub(crate) fn skip_delay(mut self) -> Self {
        self.skip_delay = true;
        self
    }

    pub(crate) fn set_error(mut self) -> Self {
        self.error = true;
        self
    }
}

#[async_trait]
pub(crate) trait WatchedTask: Send + Sync + 'static {
    async fn run_single(
        &mut self,
        app: Arc<App>,
        heartbeat: Heartbeat,
    ) -> Result<WatchedTaskOutput>;
    async fn run_single_with_timeout(
        &mut self,
        app: Arc<App>,
        heartbeat: Heartbeat,
        should_timeout: bool,
    ) -> Result<WatchedTaskOutput> {
        if should_timeout {
            match tokio::time::timeout(
                tokio::time::Duration::from_secs(MAX_TASK_SECONDS),
                self.run_single(app, heartbeat),
            )
            .await
            {
                Ok(x) => x,
                Err(e) => Err(anyhow::anyhow!(
                    "Running a single task took too long, killing. Elapsed time: {e}"
                )),
            }
        } else {
            self.run_single(app, heartbeat).await
        }
    }
}

const MAX_TASK_SECONDS: u64 = 180;

pub(crate) struct Heartbeat {
    task_status: Arc<RwLock<TaskStatus>>,
}

impl Heartbeat {
    pub(crate) async fn reset_too_old(&self) {
        let mut guard = self.task_status.write().await;
        let old = &*guard;
        *guard = TaskStatus {
            last_result: old.last_result.clone(),
            last_retry_error: old.last_retry_error.clone(),
            current_run_started: Some(Utc::now()),
            out_of_date: old.out_of_date,
            counts: old.counts,
            expire_last_result: old.expire_last_result,
        };
    }
}

#[async_trait]
pub(crate) trait WatchedTaskPerMarket: Send + Sync + 'static {
    async fn run_single_market(
        &mut self,
        app: &App,
        factory_info: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput>;
}

#[async_trait]
impl<T: WatchedTaskPerMarket> WatchedTask for T {
    async fn run_single(
        &mut self,
        app: Arc<App>,
        heartbeat: Heartbeat,
    ) -> Result<WatchedTaskOutput> {
        let factory = app.get_factory_info().await;
        let mut successes = vec![];
        let mut errors = vec![];
        let mut total_skip_delay = false;
        for market in &factory.markets {
            let market_start_time = Utc::now();
            let res = self.run_single_market(&app, &factory, market).await;
            let time_used = Utc::now() - market_start_time;
            tracing::debug!("Time used for market {}: {time_used}.", market.market_id);
            match res {
                Ok(WatchedTaskOutput {
                    skip_delay,
                    message,
                    suppress,
                    expire_alert: _,
                    error,
                }) => {
                    if suppress {
                        errors.push(format!("Found a 'suppress' which is not supported for per-market updates: {message}"));
                    }
                    if error {
                        errors.push(message.into_owned());
                    } else {
                        successes.push(format!(
                            "{market} {addr}: {message}",
                            market = market.market_id,
                            addr = market.market
                        ));
                    }
                    total_skip_delay = skip_delay || total_skip_delay;
                }
                Err(e) => errors.push(format!(
                    "{market} {addr}: {e:?}",
                    market = market.market_id,
                    addr = market.market
                )),
            }
            heartbeat.reset_too_old().await;
        }
        if errors.is_empty() {
            Ok(WatchedTaskOutput {
                skip_delay: total_skip_delay,
                message: successes.join("\n").into(),
                suppress: false,
                expire_alert: None,
                error: false,
            })
        } else {
            let mut msg = String::new();
            for line in errors.iter().chain(successes.iter()) {
                msg += line;
                msg += "\n";
            }
            Err(anyhow::anyhow!("{msg}"))
        }
    }
}

#[async_trait]
pub(crate) trait WatchedTaskPerMarketParallel: Send + Sync + 'static {
    async fn run_single_market(
        self: Arc<Self>,
        app: &App,
        factory_info: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput>;
}

pub(crate) struct ParallelWatcher<T>(Arc<T>);

impl<T> ParallelWatcher<T> {
    pub(crate) fn new(t: T) -> Self {
        ParallelWatcher(Arc::new(t))
    }
}

#[async_trait]
impl<T: WatchedTaskPerMarketParallel> WatchedTask for ParallelWatcher<T> {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        let factory = app.get_factory_info().await;
        let mut successes = vec![];
        let mut errors = vec![];
        let mut total_skip_delay = false;

        let mut set = JoinSet::new();
        for market in &factory.markets {
            let factory = factory.clone();
            let market = market.clone();
            let inner = self.0.clone();
            let app = app.clone();
            set.spawn(async move {
                let market_start_time = Utc::now();
                let res = inner.run_single_market(&app, &factory, &market).await;
                let time_used = Utc::now() - market_start_time;
                tracing::debug!("Time used for market {}: {time_used}.", market.market_id);
                (market, res)
            });
        }

        while let Some(res_outer) = set.join_next().await {
            match res_outer {
                Ok((market, res)) => match res {
                    Ok(WatchedTaskOutput {
                        skip_delay,
                        message,
                        suppress,
                        expire_alert: _,
                        error,
                    }) => {
                        if suppress {
                            errors.push(format!("Found a 'suppress' which is not supported for per-market updates: {message}"));
                        }
                        if error {
                            errors.push(message.into_owned());
                        } else {
                            successes.push(format!(
                                "{market} {addr}: {message}",
                                market = market.market_id,
                                addr = market.market
                            ));
                        }
                        total_skip_delay = skip_delay || total_skip_delay;
                    }
                    Err(e) => errors.push(format!(
                        "{market} {addr}: {e:?}",
                        market = market.market_id,
                        addr = market.market
                    )),
                },
                Err(panic) => errors.push(format!(
                    "Code bug, panic occurred in parallel market watcher: {panic:?}"
                )),
            }
        }
        if errors.is_empty() {
            Ok(WatchedTaskOutput {
                skip_delay: total_skip_delay,
                message: successes.join("\n").into(),
                suppress: false,
                expire_alert: None,
                error: false,
            })
        } else {
            let mut msg = String::new();
            for line in errors.iter().chain(successes.iter()) {
                msg += line;
                msg += "\n";
            }
            Err(anyhow::anyhow!("{msg}"))
        }
    }
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
struct RenderedStatus {
    label: TaskLabel,
    status: TaskStatus,
    short: ShortStatus,
}

impl TaskStatuses {
    async fn statuses(&self, selected_label: Option<TaskLabel>) -> Vec<RenderedStatus> {
        let mut all_statuses = vec![];
        for (label, status) in self
            .statuses
            .iter()
            .filter(|(curr_label, _)| match selected_label {
                None => true,
                Some(label) => **curr_label == label,
            })
        {
            let label = *label;
            let status = status.read().await.clone();
            let short = status.short(label, selected_label);
            all_statuses.push(RenderedStatus {
                label,
                status,
                short,
            });
        }

        all_statuses.sort_by_key(|x| (x.short, x.label));
        all_statuses
    }

    pub(crate) async fn statuses_html(
        &self,
        app: &App,
        label: Option<TaskLabel>,
    ) -> axum::response::Response {
        let template = self.to_template(app, label).await;
        let mut res = template.render().unwrap().into_response();
        res.headers_mut().insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );

        if template.alert {
            let failure_status = template
                .statuses
                .iter()
                .filter(|x| x.short.alert())
                .collect::<Vec<_>>();
            tracing::error!("Status failure: {:#?}", failure_status);
            *res.status_mut() = http::status::StatusCode::INTERNAL_SERVER_ERROR;
        }

        res
    }

    pub(crate) async fn statuses_json(
        &self,
        app: &App,
        label: Option<TaskLabel>,
    ) -> axum::response::Response {
        let template = self.to_template(app, label).await;

        let mut res = Json(&template).into_response();

        if template.alert {
            let failure_status = template
                .statuses
                .iter()
                .filter(|x| x.short.alert())
                .collect::<Vec<_>>();
            tracing::error!("Status failure: {:#?}", failure_status);
            *res.status_mut() = http::status::StatusCode::INTERNAL_SERVER_ERROR;
        }

        res
    }

    pub(crate) async fn statuses_text(
        &self,
        app: &App,
        label: Option<TaskLabel>,
    ) -> axum::response::Response {
        let mut response_builder = ResponseBuilder {
            buffer: format!("{}\n\n", app.cosmos.node_health_report()),
            any_errors: false,
        };
        let statuses = self.statuses(label).await;
        let alert = statuses.iter().any(|x| x.short.alert());

        statuses
            .iter()
            .for_each(|rendered| response_builder.add(rendered.clone()).unwrap());
        let mut res = response_builder.into_response();

        if alert {
            let failure_status = statuses
                .iter()
                .filter(|x| x.short.alert())
                .collect::<Vec<_>>();
            tracing::error!("Status failure: {:#?}", failure_status);
            *res.status_mut() = http::status::StatusCode::INTERNAL_SERVER_ERROR;
        }

        res
    }
}

struct ResponseBuilder {
    buffer: String,
    any_errors: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, Debug)]
#[serde(rename_all = "kebab-case")]
enum ShortStatus {
    Error,
    OutOfDateError,
    OutOfDate,
    ErrorNoAlert,
    OutOfDateNoAlert,
    Success,
    NotYetRun,
}

impl TaskStatus {
    fn short(&self, label: TaskLabel, selected_label: Option<TaskLabel>) -> ShortStatus {
        match self.last_result.value.as_ref() {
            TaskResultValue::Ok(_) => {
                match (self.is_out_of_date(), label.triggers_alert(selected_label)) {
                    (OutOfDateType::Not, _) => ShortStatus::Success,
                    (_, false) => ShortStatus::OutOfDateNoAlert,
                    (OutOfDateType::Slightly, true) => ShortStatus::OutOfDate,
                    (OutOfDateType::Very, true) => ShortStatus::OutOfDateError,
                }
            }
            TaskResultValue::Err(_) => {
                if label.triggers_alert(selected_label) {
                    if self.is_expired() {
                        ShortStatus::ErrorNoAlert
                    } else {
                        ShortStatus::Error
                    }
                } else {
                    ShortStatus::ErrorNoAlert
                }
            }
            TaskResultValue::NotYetRun => ShortStatus::NotYetRun,
        }
    }
}

impl ShortStatus {
    fn as_str(self) -> &'static str {
        match self {
            ShortStatus::OutOfDate => "OUT OF DATE",
            ShortStatus::OutOfDateError => "ERROR DUE TO OUT OF DATE",
            ShortStatus::OutOfDateNoAlert => "OUT OF DATE (no alert)",
            ShortStatus::Success => "SUCCESS",
            ShortStatus::Error => "ERROR",
            ShortStatus::ErrorNoAlert => "ERROR (no alert)",
            ShortStatus::NotYetRun => "NOT YET RUN",
        }
    }

    fn alert(&self) -> bool {
        match self {
            ShortStatus::Error => true,
            ShortStatus::OutOfDateError => true,
            ShortStatus::OutOfDate => false,
            ShortStatus::ErrorNoAlert => false,
            ShortStatus::OutOfDateNoAlert => false,
            ShortStatus::Success => false,
            ShortStatus::NotYetRun => false,
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            ShortStatus::Error => "link-danger",
            ShortStatus::OutOfDateError => "link-danger",
            ShortStatus::OutOfDate => "text-red-400",
            ShortStatus::ErrorNoAlert => "text-red-400",
            ShortStatus::OutOfDateNoAlert => "text-red-300",
            ShortStatus::Success => "link-success",
            ShortStatus::NotYetRun => "link-primary",
        }
    }
}

impl ResponseBuilder {
    fn add(
        &mut self,
        RenderedStatus {
            label,
            status:
                TaskStatus {
                    last_result,
                    last_retry_error,
                    current_run_started,
                    out_of_date: _,
                    counts: _,
                    expire_last_result: _,
                },
            short,
        }: RenderedStatus,
    ) -> std::fmt::Result {
        writeln!(&mut self.buffer, "# {label}. Status: {}", short.as_str())?;

        if let Some(started) = current_run_started {
            writeln!(&mut self.buffer, "Currently running, started at {started}")?;
        }

        writeln!(&mut self.buffer)?;
        match last_result.value.as_ref() {
            TaskResultValue::Ok(msg) => {
                writeln!(&mut self.buffer, "{msg}")?;
            }
            TaskResultValue::Err(err) => {
                writeln!(&mut self.buffer, "{err}")?;
            }
            TaskResultValue::NotYetRun => writeln!(&mut self.buffer, "{}", NOT_YET_RUN_MESSAGE)?,
        }
        writeln!(&mut self.buffer)?;

        if let Some(err) = last_retry_error {
            writeln!(&mut self.buffer)?;
            writeln!(
                &mut self.buffer,
                "Currently retrying, last attempt failed with:\n\n{}",
                err.value
            )?;
            writeln!(&mut self.buffer)?;
        }

        writeln!(&mut self.buffer)?;
        Ok(())
    }

    fn into_response(self) -> axum::response::Response {
        let mut res = self.buffer.into_response();
        if self.any_errors {
            *res.status_mut() = http::status::StatusCode::INTERNAL_SERVER_ERROR;
        }
        res
    }
}

impl TaskResult {
    fn since(&self) -> Since {
        Since(self.updated)
    }
}

impl TaskError {
    fn since(&self) -> Since {
        Since(self.updated)
    }
}

struct Since(DateTime<Utc>);

impl Display for Since {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let duration = Utc::now().signed_duration_since(self.0);
        let secs = duration.num_seconds();

        match secs.cmp(&0) {
            std::cmp::Ordering::Less => write!(f, "{}", self.0),
            std::cmp::Ordering::Equal => write!(f, "just now ({})", self.0),
            std::cmp::Ordering::Greater => {
                let minutes = secs / 60;
                let secs = secs % 60;
                let hours = minutes / 60;
                let minutes = minutes % 60;
                let days = hours / 24;
                let hours = hours % 24;

                let mut need_space = false;
                for (number, letter) in [(days, 'd'), (hours, 'h'), (minutes, 'm'), (secs, 's')] {
                    if number > 0 {
                        if need_space {
                            write!(f, " {number}{letter}")?;
                        } else {
                            need_space = true;
                            write!(f, "{number}{letter}")?;
                        }
                    }
                }

                write!(f, " ({})", self.0)
            }
        }
    }
}

use askama::Template;
#[derive(Template, serde::Serialize)]
#[template(path = "status.html")]
#[serde(rename_all = "kebab-case")]
struct StatusTemplate<'a> {
    statuses: Vec<RenderedStatus>,
    family: Cow<'a, str>,
    build_version: &'a str,
    grpc: String,
    frontend_info_testnet: Option<Arc<FrontendInfoTestnet>>,
    live_since: DateTime<Utc>,
    now: DateTime<Utc>,
    alert: bool,
    node_health: Vec<String>,
    gas_multiplier: f64,
    gas_multiplier_gas_check: f64,
    max_gas_prices: Option<MaxGasPrices>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
struct MaxGasPrices {
    alert_congested: f64,
    max_price: f64,
    high_max_price: f64,
    very_high_max_price: f64,
}

impl TaskStatuses {
    async fn to_template<'a>(
        &'a self,
        app: &'a App,
        label: Option<TaskLabel>,
    ) -> StatusTemplate<'a> {
        let statuses = self.statuses(label).await;
        let alert = statuses.iter().any(|x| x.short.alert());
        let frontend_info_testnet = app.get_frontend_info_testnet().await;
        let max_gas_prices = match &app.config.by_type {
            crate::config::BotConfigByType::Testnet { .. } => None,
            crate::config::BotConfigByType::Mainnet { inner } => Some(MaxGasPrices {
                alert_congested: inner.gas_price_congested,
                max_price: inner.max_gas_price,
                high_max_price: inner.higher_max_gas_price,
                very_high_max_price: inner.higher_very_high_max_gas_price,
            }),
        };
        StatusTemplate {
            statuses,
            family: match &app.config.by_type {
                crate::config::BotConfigByType::Testnet { inner } => {
                    (&inner.contract_family).into()
                }
                crate::config::BotConfigByType::Mainnet { inner } => {
                    format!("Factory address {}", inner.factory).into()
                }
            },
            build_version: build_version(),
            grpc: app.cosmos.get_cosmos_builder().grpc_url().to_owned(),
            frontend_info_testnet,
            live_since: app.live_since,
            now: Utc::now(),
            alert,
            node_health: app
                .cosmos
                .node_health_report()
                .nodes
                .into_iter()
                .map(|item| item.to_string())
                .collect(),
            gas_multiplier: app.cosmos.get_current_gas_multiplier(),
            gas_multiplier_gas_check: app.cosmos_gas_check.get_current_gas_multiplier(),
            max_gas_prices,
        }
    }
}
