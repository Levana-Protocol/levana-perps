use std::{fmt::Write, sync::Arc, time::Instant};

use crate::{
    util::oracle::OffchainPriceData,
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};

use super::{
    gas_check::GasCheckWallet, price::price_get_update_oracles_msg, App, AppBuilder,
    CrankTriggerReason,
};
use anyhow::{bail, Context, Result};
use axum::async_trait;
use cosmos::{Address, HasAddress, TxBuilder, Wallet};
use msg::contracts::market::entry::ExecuteMsg as MarketExecuteMsg;
use parking_lot::Mutex;
use shared::storage::MarketId;

// For high gas, we only care about whether there is a current work item.
// We don't need a queue of historical work items to process
// But they do get appended into a single work item
pub struct HighGasTrigger {
    current_work: Arc<Mutex<Option<HighGasWork>>>,
    sender: async_channel::Sender<()>,
}

impl HighGasTrigger {
    pub(crate) async fn set(&self, work: HighGasWork) {
        // explicit scope to drop the lock
        {
            let lock = &mut *self.current_work.lock();
            if let Some(prev) = lock.take() {
                *lock = Some(prev.append(work));
            } else {
                *lock = Some(work);
            }
        };
        let _ = self.sender.send(()).await;
    }
}
pub(crate) enum HighGasWork {
    Price {
        offchain_price_data: Arc<OffchainPriceData>,
        markets_to_update: Vec<(Address, MarketId, CrankTriggerReason)>,
        queued: Instant,
    },
}

impl HighGasWork {
    pub fn append(self, other: Self) -> Self {
        match (self, other) {
            (
                HighGasWork::Price {
                    offchain_price_data,
                    mut markets_to_update,
                    queued: queued1,
                },
                HighGasWork::Price {
                    offchain_price_data: other_offchain_price_data,
                    markets_to_update: other_markets_to_update,
                    queued: queued2,
                },
            ) => {
                for (market, market_id, reason) in other_markets_to_update.into_iter() {
                    if !markets_to_update.iter().any(|(_, id, _)| *id == market_id) {
                        markets_to_update.push((market, market_id, reason));
                    }
                }

                // would be nice to get rid of this clone, but this path shouldn't be hit very often
                let mut offchain_price_data = (*offchain_price_data).clone();
                let OffchainPriceData {
                    values,
                    stable_ids,
                    edge_ids,
                } = &*other_offchain_price_data;

                offchain_price_data.stable_ids.extend(stable_ids.iter());
                offchain_price_data.edge_ids.extend(edge_ids.iter());

                for (key, value) in values {
                    // insert if it's a brand new key or if the timestamp is newer than the previous one
                    let should_insert = match offchain_price_data.values.get(key) {
                        None => true,
                        Some(prev) if value.1 >= prev.1 => true,
                        _ => false,
                    };

                    if should_insert {
                        offchain_price_data.values.insert(*key, *value);
                    }
                }

                HighGasWork::Price {
                    offchain_price_data: Arc::new(offchain_price_data),
                    markets_to_update,
                    queued: queued1.min(queued2),
                }
            }
        }
    }
}

/// Start the background thread to run "high gas" tasks.
impl AppBuilder {
    pub(super) fn start_high_gas(&mut self) -> Result<HighGasTrigger> {
        let wallet = self
            .app
            .config
            .high_gas_wallet
            .clone()
            .context("high gas wallet is required")?;
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
    wallet: Arc<Wallet>,
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

        let work = self
            .current_work
            .try_lock()
            .and_then(|mut item| item.take());
        let mut successes = vec![];

        match work {
            Some(work) => match work {
                HighGasWork::Price {
                    offchain_price_data,
                    markets_to_update,
                    queued,
                } => {
                    let received = Instant::now();
                    successes.push(format!(
                        "Received new work, delta between queued and now: {:?}",
                        queued.elapsed(),
                    ));
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
                            &*self.wallet,
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
                                successes.push(format!("[VERY HIGH GAS] - we think we're in the Osmosis epoch, error: {e:?}"));
                            } else if app.get_congested_info().is_congested() {
                                bail!("[VERY HIGH GAS] - we think the Osmosis chain is overly congested, error: {e:?}, delta between queued and now: {:?}, delta between received and now: {:?}",
                                    queued.elapsed(),
                                    received.elapsed(),
                                );
                            } else {
                                let error_as_str = format!("{e:?}");
                                if error_as_str.contains("out of gas")
                                    || error_as_str.contains("code 11")
                                {
                                    bail!("[VERY HIGH GAS] - Got an 'out of gas' code 11 when trying to crank. error: {e:?}, delta between queued and now: {:?}, delta between received and now: {:?}",
                                        queued.elapsed(),
                                        received.elapsed(),
                                    );
                                } else {
                                    bail!("[VERY HIGH GAS]\n{:?}\nDelta between queued and now: {:?}\nDelta between received and now: {:?}",
                                        e,
                                        queued.elapsed(),
                                        received.elapsed(),
                                    );
                                }
                            }
                        }
                    }

                    successes.push(format!(
                        "Finished the work, delta between queued and now: {:?}, delta between received and now: {:?}",
                        queued.elapsed(),
                        received.elapsed(),
                    ));
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
