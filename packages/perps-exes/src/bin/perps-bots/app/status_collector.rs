mod gas_funds;

use std::{
    collections::HashMap,
    fmt::Write,
    sync::{Arc, Weak},
};

use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use cosmos::{Cosmos, CosmosNetwork};
use parking_lot::RwLock;
use reqwest::StatusCode;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, serde::Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StatusCategory {
    Price,
    Crank,
    GasCheck,
    Nibb,
    GetFactory,
}

impl StatusCategory {
    fn all() -> [StatusCategory; 5] {
        [
            StatusCategory::Price,
            StatusCategory::Crank,
            StatusCategory::GasCheck,
            StatusCategory::Nibb,
            StatusCategory::GetFactory,
        ]
    }
}

#[derive(Clone)]
pub(crate) struct StatusCollector {
    pub(crate) collections: Arc<RwLock<StatusMap>>,
    pub(crate) cosmos_network: CosmosNetwork,
    pub(crate) client: reqwest::Client,
    pub(crate) cosmos: Cosmos,
}

pub(crate) struct WeakStatusCollector {
    collections: Weak<RwLock<StatusMap>>,
    cosmos_network: CosmosNetwork,
    client: reqwest::Client,
    cosmos: Cosmos,
}

type StatusMap = HashMap<StatusCategory, HashMap<String, Status>>;

impl StatusCollector {
    pub(crate) fn add_status(
        &self,
        category: StatusCategory,
        key: impl Into<String>,
        status: Status,
    ) {
        let key = key.into();
        self.collections
            .write()
            .entry(category)
            .or_default()
            .insert(key, status);
    }

    pub(crate) fn all(&self) -> axum::response::Response {
        let collections = self.collections.read();
        let mut response_builder = ResponseBuilder::default();
        for category in StatusCategory::all() {
            response_builder
                .add(category, collections.get(&category))
                .unwrap();
        }
        response_builder.into_response()
    }

    pub(crate) fn single(&self, category: StatusCategory) -> axum::response::Response {
        let collections = self.collections.read();
        let mut response_builder = ResponseBuilder::default();
        response_builder
            .add(category, collections.get(&category))
            .unwrap();
        response_builder.into_response()
    }

    pub(crate) fn downgrade(&self) -> WeakStatusCollector {
        WeakStatusCollector {
            collections: Arc::downgrade(&self.collections),
            cosmos_network: self.cosmos_network,
            client: self.client.clone(),
            cosmos: self.cosmos.clone(),
        }
    }

    pub(crate) fn add_status_check<F, Fut>(
        &self,
        category: StatusCategory,
        key: impl Into<String>,
        delay_seconds: u64,
        action: F,
    ) where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Status> + Send,
    {
        let weak = self.downgrade();
        let key = key.into();
        tokio::task::spawn(async move {
            while let Some(status_collector) = weak.upgrade() {
                let status = retry(&action, is_success_single).await;
                status_collector.add_status(category, key.clone(), status);
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds)).await;
            }
        });
    }

    pub(crate) fn add_status_checks<F, Fut>(
        &self,
        category: StatusCategory,
        delay_seconds: u64,
        action: F,
    ) where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Vec<(String, Status)>> + Send,
    {
        let weak = self.downgrade();
        tokio::task::spawn(async move {
            while let Some(status_collector) = weak.upgrade() {
                for (key, status) in retry(&action, |x| is_success_multi(x)).await {
                    status_collector.add_status(category, key, status);
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds)).await;
            }
        });
    }
}

impl WeakStatusCollector {
    pub(crate) fn upgrade(&self) -> Option<StatusCollector> {
        self.collections
            .upgrade()
            .map(|collections| StatusCollector {
                collections,
                cosmos_network: self.cosmos_network,
                client: self.client.clone(),
                cosmos: self.cosmos.clone(),
            })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Status {
    status_type: StatusType,
    message: String,
    updated: DateTime<Utc>,
}

#[derive(Clone, Debug)]
enum StatusType {
    Error,
    Success { valid_for_seconds: Option<i64> },
}

impl Status {
    pub(crate) fn error(message: impl Into<String>) -> Self {
        let message = message.into();
        log::error!("New error status detected: {message}");
        Status {
            status_type: StatusType::Error,
            message,
            updated: Utc::now(),
        }
    }

    pub(crate) fn success(message: impl Into<String>, valid_for_seconds: Option<i64>) -> Self {
        let message = message.into();
        log::debug!("New success status detected: {message}");
        Status {
            status_type: StatusType::Success { valid_for_seconds },
            message,
            updated: Utc::now(),
        }
    }
}

fn is_out_of_date(updated: DateTime<Utc>, valid_for_seconds: Option<i64>) -> bool {
    match valid_for_seconds {
        Some(valid_for_seconds) => {
            let delta = Utc::now() - updated;
            delta.num_seconds() > valid_for_seconds
        }
        None => false,
    }
}

#[derive(Default)]
struct ResponseBuilder {
    buffer: String,
    any_errors: bool,
}

impl ResponseBuilder {
    fn add(
        &mut self,
        category: StatusCategory,
        vals: Option<&HashMap<String, Status>>,
    ) -> std::fmt::Result {
        writeln!(&mut self.buffer, "# {category:?}")?;
        match vals {
            Some(vals) if !vals.is_empty() => {
                for (
                    key,
                    Status {
                        status_type,
                        message,
                        updated,
                    },
                ) in vals
                {
                    writeln!(&mut self.buffer)?;
                    writeln!(&mut self.buffer, "## Status key: {key}")?;
                    writeln!(&mut self.buffer)?;
                    match status_type {
                        StatusType::Error => {
                            writeln!(&mut self.buffer, "**ERROR**")?;
                            writeln!(&mut self.buffer)?;
                            self.any_errors = true;
                        }
                        StatusType::Success { valid_for_seconds } => {
                            if is_out_of_date(*updated, *valid_for_seconds) {
                                writeln!(&mut self.buffer, "*** OUT OF DATE***")?;
                                writeln!(&mut self.buffer)?;
                                self.any_errors = true;
                            }
                        }
                    }
                    writeln!(&mut self.buffer, "```")?;
                    writeln!(&mut self.buffer, "{}", message)?;
                    writeln!(&mut self.buffer, "```")?;
                    writeln!(&mut self.buffer)?;
                    writeln!(&mut self.buffer, "Last updated: {}", updated)?;
                }
            }
            _ => {
                self.any_errors = true;
                writeln!(&mut self.buffer)?;
                writeln!(&mut self.buffer, "**NO STATUSES FOUND**")?;
            }
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

async fn retry<F, Fut, IsSuccess>(mk_fut: &F, is_success: IsSuccess) -> Fut::Output
where
    F: Fn() -> Fut,
    Fut: std::future::Future,
    IsSuccess: Fn(&Fut::Output) -> bool,
{
    for _ in 0..4 {
        let x = mk_fut().await;
        if is_success(&x) {
            return x;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
    }
    mk_fut().await
}

fn is_success_single(status: &Status) -> bool {
    match status.status_type {
        StatusType::Error => {
            log::warn!("Error when updating status, retrying: {}", status.message);
            false
        }
        StatusType::Success {
            valid_for_seconds: _,
        } => true,
    }
}

fn is_success_multi<T>(statuses: &[(T, Status)]) -> bool {
    statuses.iter().all(|(_, x)| is_success_single(x))
}
