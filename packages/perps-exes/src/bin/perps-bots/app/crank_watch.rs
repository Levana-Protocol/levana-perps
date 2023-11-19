use std::borrow::Cow;
use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use chrono::{Duration, Utc};
use cosmos::HasAddress;
use msg::contracts::market::crank::CrankWorkInfo;
use perps_exes::prelude::MarketContract;

use crate::util::markets::Market;
use crate::watcher::{ParallelWatcher, WatchedTaskOutput, WatchedTaskPerMarketParallel};

use super::factory::FactoryInfo;
use super::App;
use super::{crank_run::TriggerCrank, AppBuilder};

#[derive(Clone)]
struct Worker {
    trigger_crank: TriggerCrank,
}

/// Start the background thread to turn the crank on the crank bots.
impl AppBuilder {
    pub(super) fn start_crank_watch(&mut self, trigger_crank: TriggerCrank) -> Result<()> {
        let worker = Worker { trigger_crank };
        self.watch_periodic(
            crate::watcher::TaskLabel::CrankWatch,
            ParallelWatcher::new(worker),
        )
    }
}

#[async_trait]
impl WatchedTaskPerMarketParallel for Worker {
    async fn run_single_market(
        self: Arc<Self>,
        app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        check_market(&self.trigger_crank, app, market).await
    }
}

#[tracing::instrument(skip_all)]
async fn check_market(
    trigger_crank: &TriggerCrank,
    app: &App,
    market: &Market,
) -> Result<WatchedTaskOutput> {
    let work = match app.check_crank(&market.market).await? {
        None => return Ok(WatchedTaskOutput::new("No crank messages waiting")),
        Some(work) => work,
    };

    trigger_crank
        .trigger_crank(market.market.get_address(), market.market_id.clone())
        .await;

    Ok(WatchedTaskOutput::new(match work {
        CrankReason::WorkAvailable(work) => {
            format!("Triggering crank because work is available: {work:?}").into()
        }
        CrankReason::OldLastCrank(age) => {
            format!("Triggering crank because of old last crank, age: {age}").into()
        }
        CrankReason::NoPriorCrank => Cow::Borrowed("No crank work needed"),
    }))
}

impl App {
    #[tracing::instrument(skip_all)]
    async fn check_crank(&self, market: &MarketContract) -> Result<Option<CrankReason>> {
        let status = market.status().await?;
        if let Some(work) = status.next_crank {
            Ok(Some(CrankReason::WorkAvailable(work)))
        } else {
            match status.last_crank_completed {
                None => Ok(Some(CrankReason::NoPriorCrank)),
                Some(timestamp) => {
                    let timestamp = timestamp.try_into_chrono_datetime()?;
                    let now = Utc::now();
                    let age = now.signed_duration_since(timestamp);
                    if age.num_seconds() > MAX_CRANK_AGE {
                        Ok(Some(CrankReason::OldLastCrank(age)))
                    } else {
                        Ok(None)
                    }
                }
            }
        }
    }
}

const MAX_CRANK_AGE: i64 = 240;

#[derive(Debug)]
enum CrankReason {
    WorkAvailable(CrankWorkInfo),
    OldLastCrank(Duration),
    NoPriorCrank,
}
