use std::{fmt::Write, sync::Arc};

use crate::{
    util::oracle::OffchainPriceData,
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};

use super::{
    gas_check::GasCheckWallet, price::price_get_update_oracles_msg, App, AppBuilder,
    CrankTriggerReason,
};
use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, HasAddress, TxBuilder, Wallet};
use msg::contracts::market::entry::ExecuteMsg as MarketExecuteMsg;
use parking_lot::Mutex;
use shared::storage::MarketId;

// For high gas, we only care about whether there is a current work item.
// We don't need a queue of historical work items to process
pub struct HighGasTrigger {
    current_work: Arc<Mutex<Option<HighGasWork>>>,
    sender: async_channel::Sender<()>,
}

impl HighGasTrigger {
    pub(crate) async fn set(&self, work: HighGasWork) {
        *self.current_work.lock() = Some(work);
        let _ = self.sender.send(()).await;
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
    pub(super) fn start_high_gas(&mut self) -> Result<HighGasTrigger> {
        let wallet = self.app.config.high_gas_wallet.clone();

        self.refill_gas(wallet.get_address(), GasCheckWallet::HighGas)?;

        let current_work = Arc::new(Mutex::new(None));
        let (sender, receiver) = async_channel::bounded(100);

        let worker = Worker {
            wallet,
            current_work: current_work.clone(),
            receiver,
        };

        self.watch_periodic(crate::watcher::TaskLabel::HighGas, worker)?;

        Ok(HighGasTrigger {
            current_work,
            sender,
        })
    }
}

struct Worker {
    wallet: Wallet,
    current_work: Arc<Mutex<Option<HighGasWork>>>,
    receiver: async_channel::Receiver<()>,
}

#[async_trait]
impl WatchedTask for Worker {
    async fn run_single(&mut self, app: Arc<App>, _: Heartbeat) -> Result<WatchedTaskOutput> {
        // instead of busy-looping to check the mutex, await a future that will be woken up at the earliest of:
        // * 20 seconds
        // * the channel receiving a value (i.e. new work added)
        match tokio::time::timeout(tokio::time::Duration::from_secs(20), self.receiver.recv()).await
        {
            // Timeout occurred, not an error, just keep going with our logic
            Err(_) => (),
            // Popped a value from the queue, all good
            Ok(Ok(())) => (),
            Ok(Err(_)) => unreachable!(
                "receive_with_timeout: impossible RecvError, all sending sides have been closed"
            ),
        }

        let work = self.current_work.lock().take();
        let mut successes = vec![];

        match work {
            Some(work) => match work {
                HighGasWork::Price {
                    offchain_price_data,
                    markets_to_update,
                } => {
                    let factory = app.get_factory_info().await;

                    let mut builder = TxBuilder::default();

                    if let Some(oracle_msg) = price_get_update_oracles_msg(
                        &self.wallet,
                        &app,
                        &factory.markets,
                        &offchain_price_data,
                    )
                    .await?
                    {
                        builder.add_message(oracle_msg);
                    }

                    for (market, _, _) in markets_to_update.into_iter().take(5) {
                        let rewards = app
                            .config
                            .get_crank_rewards_wallet()
                            .map(|a| a.get_address_string().into());

                        builder.add_execute_message(
                            market,
                            &self.wallet,
                            vec![],
                            MarketExecuteMsg::Crank {
                                execs: Some(0),
                                rewards: rewards.clone(),
                            },
                        )?;
                    }

                    match builder
                        .sign_and_broadcast_cosmos_tx(&app.cosmos_very_high_gas, &self.wallet)
                        .await
                    {
                        Ok(txres) => {
                            successes.push(format!(
                                "[VERY HIGH GAS] - Successfully executed in transaction {}.",
                                txres.response.txhash
                            ));
                        }
                        Err(e) => {
                            if app.is_osmosis_epoch() {
                                successes.push(format!("[VERY HIGH GAS] - Ignoring crank run error since we think we're in the Osmosis epoch, error: {e:?}"));
                            } else if app.get_congested_info().is_congested() {
                                successes.push(format!("[VERY HIGH GAS] - Ignoring crank run error since we think the Osmosis chain is overly congested, error: {e:?}"));
                            } else {
                                let error_as_str = format!("{e:?}");

                                // If we got an "out of gas" code 11 error, we want to ignore
                                // it. This usually happens when new work comes in. The logic
                                // below to check if new work is available will cause a new
                                // crank run to be scheduled, if one is needed.
                                if error_as_str.contains("out of gas")
                                    || error_as_str.contains("code 11")
                                {
                                    successes.push("[VERY HIGH GAS] - Got an 'out of gas' code 11 when trying to crank.".to_string());
                                }
                                // We previously checked here for a price_too_old error message.
                                // However, with the new price logic from deferred execution, that should never
                                // happen. So now we'll simply allow such an error message to bubble up.
                                else {
                                    return Err(anyhow::anyhow!("[VERY HIGH GAS] - {:?}", e));
                                }
                            }
                        }
                    }
                }
            },
            None => {
                successes.push("[VERY HIGH GAS] - no work to do".to_string());
            }
        }

        let mut msg = String::new();
        for line in successes.into_iter() {
            writeln!(&mut msg, "{line}")?;
        }

        Ok(WatchedTaskOutput::new(msg))
    }
}
