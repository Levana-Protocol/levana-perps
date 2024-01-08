use std::{fmt::Write, sync::Arc};

use crate::{
    util::oracle::OffchainPriceData,
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};

use super::{
    crank_run::TriggerCrank, gas_check::GasCheckWallet, price::update_oracles, App, AppBuilder,
    CrankTriggerReason,
};
use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, HasAddress, Wallet};
use parking_lot::Mutex;
use shared::storage::MarketId;

// For high gas, we only care about whether there is a current work item.
// We don't need a queue of historical work items to process
pub struct HighGasTrigger {
    current_work: Arc<Mutex<Option<HighGasWork>>>,
}

impl HighGasTrigger {
    pub(crate) fn set(&self, work: HighGasWork) {
        *self.current_work.lock() = Some(work);
    }
}
pub(crate) enum HighGasWork {
    Price {
        offchain_price_data: Arc<OffchainPriceData>,
        markets_to_update: Vec<(Address, MarketId, CrankTriggerReason)>,
    },
}

/// Start the background thread to run "high gas" tasks.
impl AppBuilder {
    pub(super) fn start_high_gas(&mut self, trigger_crank: TriggerCrank) -> Result<HighGasTrigger> {
        let wallet = self.app.config.high_gas_wallet.clone();

        self.refill_gas(wallet.get_address(), GasCheckWallet::HighGas)?;

        let current_work = Arc::new(Mutex::new(None));

        let worker = Worker {
            wallet,
            trigger_crank,
            current_work: current_work.clone(),
        };

        self.watch_periodic(crate::watcher::TaskLabel::HighGas, worker)?;

        Ok(HighGasTrigger { current_work })
    }
}

struct Worker {
    wallet: Wallet,
    current_work: Arc<Mutex<Option<HighGasWork>>>,
    trigger_crank: TriggerCrank,
}

#[async_trait]
impl WatchedTask for Worker {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        let work = self.current_work.lock().take();
        let mut successes = vec![];

        match work {
            Some(work) => match work {
                HighGasWork::Price {
                    offchain_price_data,
                    markets_to_update,
                } => {
                    let factory = app.get_factory_info().await;
                    successes.push(
                        update_oracles(
                            &self.wallet,
                            &app,
                            &factory.markets,
                            &offchain_price_data,
                            true,
                        )
                        .await?,
                    );

                    for (market, market_id, reason) in markets_to_update {
                        self.trigger_crank
                            .trigger_crank(market, market_id, reason)
                            .await;
                    }

                    successes.push("high gas - update price success".to_string());
                }
            },
            None => {
                successes.push("high gas - no work to do".to_string());
            }
        }

        let mut msg = String::new();
        for line in successes.into_iter() {
            writeln!(&mut msg, "{line}")?;
        }

        Ok(WatchedTaskOutput::new(msg))
    }
}
