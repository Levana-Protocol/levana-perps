use std::borrow::Cow;
use std::fmt::{Display, Write};
use std::pin::Pin;
use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result};
use axum::http::HeaderValue;
use axum::response::IntoResponse;
use axum::{async_trait, Json};
use chrono::{DateTime, Duration, Utc};
use cosmos::Address;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use perps_exes::build_version;
use perps_exes::{
    config::{TaskConfig, WatcherConfig},
    prelude::MarketId,
};
use rand::Rng;
use reqwest::header::CONTENT_TYPE;
use reqwest::StatusCode;
use tokio::task::JoinSet;

use crate::app::factory::FrontendInfoTestnet;
use crate::app::AppBuilder;
use crate::app::{factory::FactoryInfo, App};

/// Different kinds of tasks that we can watch
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub(crate) enum TaskLabel {
    GetFactory,
    Stale,
    Crank,
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
}

impl TaskLabel {
    pub(crate) fn from_slug(s: &str) -> Option<TaskLabel> {
        match s {
            "get-factory" => Some(TaskLabel::GetFactory),
            "stale" => Some(TaskLabel::Stale),
            "crank" => Some(TaskLabel::Crank),
            "price" => Some(TaskLabel::Price),
            "track-balance" => Some(TaskLabel::TrackBalance),
            "stats" => Some(TaskLabel::Stats),
            "stats-alert" => Some(TaskLabel::StatsAlert),
            "gas-check" => Some(TaskLabel::GasCheck),
            "liquidity" => Some(TaskLabel::Liquidity),
            "utilization" => Some(TaskLabel::Utilization),
            "balance" => Some(TaskLabel::Balance),
            // Being lazy, skipping UltraCrank and Trader, they aren't needed
            _ => None,
        }
    }
}

impl Display for TaskLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TaskLabel::Trader { index } => write!(f, "Trader #{index}"),
            TaskLabel::UltraCrank { index } => write!(f, "Ultra crank #{index}"),
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

#[derive(Default)]
pub(crate) struct TaskStatuses {
    statuses: Arc<OnceCell<StatusMap>>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskStatus {
    last_result: TaskResult,
    last_retry_error: Option<TaskError>,
    current_run_started: Option<DateTime<Utc>>,
    #[serde(skip)]
    out_of_date: Duration,
    counts: TaskCounts,
}

#[derive(Clone, Copy, Default, serde::Serialize)]
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

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskResult {
    pub(crate) value: Arc<TaskResultValue>,
    pub(crate) updated: DateTime<Utc>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum TaskResultValue {
    Ok(String),
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

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskError {
    pub(crate) value: Arc<String>,
    pub(crate) updated: DateTime<Utc>,
}

impl TaskStatus {
    fn is_out_of_date(&self) -> bool {
        match self.current_run_started {
            Some(started) => {
                let out_of_date = started + self.out_of_date;
                out_of_date <= Utc::now()
            }
            None => false,
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
            TaskLabel::Crank => config.crank,
            TaskLabel::GetFactory => config.get_factory,
            TaskLabel::Price => config.price,
            TaskLabel::Stats => config.stats,
            TaskLabel::StatsAlert => config.stats_alert,
        }
    }

    fn triggers_alert(&self, selected_label: Option<TaskLabel>) -> bool {
        // If we loaded up a specific status page, always treat it as an alert.
        if selected_label.as_ref() == Some(self) {
            return true;
        }
        match self {
            TaskLabel::GetFactory => true,
            TaskLabel::Crank => true,
            TaskLabel::Price => true,
            TaskLabel::TrackBalance => true,
            TaskLabel::GasCheck => false,
            TaskLabel::UltraCrank { index: _ } => false,
            TaskLabel::Liquidity => false,
            TaskLabel::Utilization => false,
            TaskLabel::Balance => false,
            TaskLabel::Trader { index: _ } => false,
            TaskLabel::Stale => true,
            TaskLabel::Stats => true,
            TaskLabel::StatsAlert => false,
        }
    }

    fn ident(self) -> Cow<'static, str> {
        match self {
            TaskLabel::GetFactory => "get-factory".into(),
            TaskLabel::Crank => "crank".into(),
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
        }
    }
}

impl Watcher {
    pub(crate) async fn wait(mut self, app: &App) -> Result<()> {
        app.statuses
            .statuses
            .set(self.statuses)
            .map_err(|_| anyhow::anyhow!("app.statuses.statuses set twice"))?;
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
        let out_of_date = chrono::Duration::seconds(config.out_of_date.into());
        let task_status = Arc::new(RwLock::new(TaskStatus {
            last_result: TaskResult {
                value: TaskResultValue::NotYetRun.into(),
                updated: Utc::now(),
            },
            last_retry_error: None,
            current_run_started: None,
            out_of_date,
            counts: Default::default(),
        }));
        {
            let old = self.watcher.statuses.insert(label, task_status.clone());
            if old.is_some() {
                anyhow::bail!("Two periodic tasks with label {label:?}");
            }
        }
        let app = self.app.clone();
        let future = Box::pin(async move {
            let mut retries = 0;
            loop {
                let old_counts = {
                    let mut guard = task_status.write();
                    let old = &*guard;
                    *guard = TaskStatus {
                        last_result: old.last_result.clone(),
                        last_retry_error: old.last_retry_error.clone(),
                        current_run_started: Some(Utc::now()),
                        out_of_date,
                        counts: old.counts,
                    };
                    guard.counts
                };
                let before = tokio::time::Instant::now();
                let res = task
                    .run_single(
                        &app,
                        Heartbeat {
                            task_status: task_status.clone(),
                        },
                    )
                    .await;
                match res {
                    Ok(WatchedTaskOutput {
                        skip_delay,
                        message,
                    }) => {
                        log::info!("{label}: Success! {message}");
                        {
                            let mut guard = task_status.write();
                            let old = &*guard;
                            let title = label.to_string();
                            if label.triggers_alert(None) {
                                match &*old.last_result.value {
                                    // Was a success, still a success, do nothing
                                    TaskResultValue::Ok(_) => (),
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
                                    value: TaskResultValue::Ok(message).into(),
                                    updated: Utc::now(),
                                },
                                last_retry_error: None,
                                current_run_started: None,
                                out_of_date,
                                counts: TaskCounts {
                                    successes: old_counts.successes + 1,
                                    ..old_counts
                                },
                            };
                        }
                        retries = 0;
                        if !skip_delay {
                            match config.delay {
                                perps_exes::config::Delay::Constant(secs) => {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(secs))
                                        .await;
                                }
                                perps_exes::config::Delay::Random { low, high } => {
                                    let secs = rand::thread_rng().gen_range(low..=high);
                                    tokio::time::sleep(tokio::time::Duration::from_secs(secs))
                                        .await;
                                }
                                perps_exes::config::Delay::Interval(secs) => {
                                    if let Some(after) =
                                        before.checked_add(tokio::time::Duration::from_secs(secs))
                                    {
                                        tokio::time::sleep_until(after).await;
                                    }
                                }
                            };
                        }
                    }
                    Err(err) => {
                        log::warn!("{label}: Error: {err:?}");
                        retries += 1;
                        let max_retries = config.retries.unwrap_or(app.config.watcher.retries);
                        // We want to get to first failure quickly so we don't
                        // have a misleading success status page. So if this
                        // failed and there are no prior attempts, don't retry.
                        if retries >= max_retries || task_status.read().counts.total() == 0 {
                            retries = 0;
                            let mut guard = task_status.write();
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
                            };
                        } else {
                            {
                                let mut guard = task_status.write();
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
    pub(crate) skip_delay: bool,
    pub(crate) message: String,
}

#[async_trait]
pub(crate) trait WatchedTask: Send + Sync + 'static {
    async fn run_single(&mut self, app: &App, heartbeat: Heartbeat) -> Result<WatchedTaskOutput>;
}

pub(crate) struct Heartbeat {
    task_status: Arc<RwLock<TaskStatus>>,
}

impl Heartbeat {
    pub(crate) fn reset_too_old(&self) {
        let mut guard = self.task_status.write();
        let old = &*guard;
        *guard = TaskStatus {
            last_result: old.last_result.clone(),
            last_retry_error: old.last_retry_error.clone(),
            current_run_started: Some(Utc::now()),
            out_of_date: old.out_of_date,
            counts: old.counts,
        };
    }
}

#[async_trait]
pub(crate) trait WatchedTaskPerMarket: Send + Sync + 'static {
    async fn run_single_market(
        &mut self,
        app: &App,
        factory_info: &FactoryInfo,
        market: &MarketId,
        addr: Address,
    ) -> Result<WatchedTaskOutput>;
}

#[async_trait]
impl<T: WatchedTaskPerMarket> WatchedTask for T {
    async fn run_single(&mut self, app: &App, heartbeat: Heartbeat) -> Result<WatchedTaskOutput> {
        let factory = app.get_factory_info();
        let mut successes = vec![];
        let mut errors = vec![];
        let mut total_skip_delay = false;
        for (market, addr) in &factory.markets {
            match self.run_single_market(app, &factory, market, *addr).await {
                Ok(WatchedTaskOutput {
                    skip_delay,
                    message,
                }) => {
                    successes.push(format!("{market} {addr}: {message}"));
                    total_skip_delay = skip_delay || total_skip_delay;
                }
                Err(e) => errors.push(format!("{market} {addr}: {e:?}")),
            }
            heartbeat.reset_too_old();
        }
        if errors.is_empty() {
            Ok(WatchedTaskOutput {
                skip_delay: total_skip_delay,
                message: successes.join("\n"),
            })
        } else {
            Err(anyhow::anyhow!("{}", errors.join("\n")))
        }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
struct RenderedStatus {
    label: TaskLabel,
    status: TaskStatus,
    short: ShortStatus,
}

impl TaskStatuses {
    fn statuses(&self, selected_label: Option<TaskLabel>) -> Vec<RenderedStatus> {
        let mut all_statuses = self
            .statuses
            .get()
            .expect("Status map isn't available yet")
            .iter()
            .filter(|(curr_label, _)| match selected_label {
                None => true,
                Some(label) => **curr_label == label,
            })
            .map(|(label, status)| {
                let label = *label;
                let status = status.read().clone();
                let short = status.short(label, selected_label);
                RenderedStatus {
                    label,
                    status,
                    short,
                }
            })
            .collect::<Vec<_>>();

        all_statuses.sort_by_key(|x| (x.short, x.label));
        all_statuses
    }

    pub(crate) fn statuses_html(
        &self,
        app: &App,
        label: Option<TaskLabel>,
    ) -> axum::response::Response {
        let template = self.to_template(app, label);
        let mut res = template.render().unwrap().into_response();
        res.headers_mut().append(
            CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );

        if template.alert {
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        }

        res
    }

    pub(crate) fn statuses_json(
        &self,
        app: &App,
        label: Option<TaskLabel>,
    ) -> axum::response::Response {
        let template = self.to_template(app, label);

        let mut res = Json(&template).into_response();

        if template.alert {
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        }

        res
    }

    pub(crate) fn statuses_text(&self, label: Option<TaskLabel>) -> axum::response::Response {
        let mut response_builder = ResponseBuilder::default();
        let statuses = self.statuses(label);
        let alert = statuses.iter().any(|x| x.short.alert());
        statuses
            .into_iter()
            .for_each(|rendered| response_builder.add(rendered).unwrap());
        let mut res = response_builder.into_response();

        if alert {
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        }

        res
    }
}

#[derive(Default)]
struct ResponseBuilder {
    buffer: String,
    any_errors: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum ShortStatus {
    Error,
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
                if self.is_out_of_date() {
                    if label.triggers_alert(selected_label) {
                        ShortStatus::OutOfDate
                    } else {
                        ShortStatus::OutOfDateNoAlert
                    }
                } else {
                    ShortStatus::Success
                }
            }
            TaskResultValue::Err(_) => {
                if label.triggers_alert(selected_label) {
                    ShortStatus::Error
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
            ShortStatus::OutOfDate => true,
            ShortStatus::ErrorNoAlert => false,
            ShortStatus::OutOfDateNoAlert => false,
            ShortStatus::Success => false,
            ShortStatus::NotYetRun => false,
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            ShortStatus::Error => "error",
            ShortStatus::OutOfDate => "out-of-date",
            ShortStatus::ErrorNoAlert => "error-no-alert",
            ShortStatus::OutOfDateNoAlert => "out-of-date-no-alert",
            ShortStatus::Success => "success",
            ShortStatus::NotYetRun => "not-yet-run",
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
                },
            short,
        }: RenderedStatus,
    ) -> std::fmt::Result {
        writeln!(&mut self.buffer, "# {label:?}. Status: {}", short.as_str())?;

        if let Some(started) = current_run_started {
            writeln!(&mut self.buffer, "Currently running, started at {started}")?;
        }

        writeln!(&mut self.buffer)?;
        match last_result.value.as_ref() {
            TaskResultValue::Ok(msg) => {
                writeln!(&mut self.buffer, "{msg}")?;
            }
            TaskResultValue::Err(err) => {
                writeln!(&mut self.buffer, "{err:?}")?;
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
            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
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
}

impl TaskStatuses {
    fn to_template<'a>(&'a self, app: &'a App, label: Option<TaskLabel>) -> StatusTemplate<'a> {
        let statuses = self.statuses(label);
        let alert = statuses.iter().any(|x| x.short.alert());
        let frontend_info_testnet = app.get_frontend_info_testnet();
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
            grpc: app.cosmos.get_first_builder().grpc_url.clone(),
            frontend_info_testnet,
            live_since: app.live_since,
            now: Utc::now(),
            alert,
        }
    }
}
