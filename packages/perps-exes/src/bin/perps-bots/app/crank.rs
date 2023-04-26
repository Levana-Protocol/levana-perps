use std::sync::Arc;

use anyhow::Result;
use cosmos::proto::cosmwasm::wasm::v1::MsgExecuteContract;
use cosmos::{Address, Cosmos, HasAddress, TxBuilder, Wallet};
use msg::contracts::market;
use parking_lot::RwLock;
use perps_exes::config::DeploymentConfig;
use tokio::sync::Mutex;

use crate::endpoints::epochs::Epochs;
use crate::util::markets::{get_markets, Market};

use super::factory::FactoryInfo;
use super::status_collector::{Status, StatusCategory, StatusCollector};

struct Worker {
    cosmos: Cosmos,
    epochs: Epochs,
    /// Fallback list of crank messages to try
    crank_messages: Arc<Vec<Vec<u8>>>,
    config: Arc<DeploymentConfig>,
    factory: Arc<RwLock<Arc<FactoryInfo>>>,
}

/// Maximum amount of simulated gas to allow for a crank message.
///
/// See: <https://phobosfinance.atlassian.net/browse/PERP-349>
const MAX_CRANK_GAS: u64 = 17_000_000;

/// Start the background thread to turn the crank on the crank bots.
impl StatusCollector {
    pub(super) async fn start_crank_bot(
        &self,
        cosmos: Cosmos,
        config: Arc<DeploymentConfig>,
        epochs: Epochs,
        factory: Arc<RwLock<Arc<FactoryInfo>>>,
        gas_wallet: Arc<Mutex<Wallet>>,
    ) -> Result<()> {
        config
            .crank_wallets
            .iter()
            .enumerate()
            .for_each(|(idx, wallet)| {
                self.track_gas_funds(
                    *wallet.address(),
                    format!("crank-bot-{idx}"),
                    config.min_gas.crank,
                    gas_wallet.clone(),
                )
            });

        let worker = Arc::new(Worker {
            cosmos,
            epochs,
            crank_messages: [100, 50, 25, 10, 5, 3, 1]
                .into_iter()
                .map(|execs| {
                    serde_json::to_vec(&market::entry::ExecuteMsg::Crank {
                        execs: Some(execs),
                        rewards: None,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            config,
            factory,
        });

        self.add_status_checks(StatusCategory::Crank, UPDATE_DELAY_SECONDS, move || {
            worker.clone().single_update()
        });

        Ok(())
    }
}

const UPDATE_DELAY_SECONDS: u64 = 1;
const TOO_OLD_SECONDS: i64 = 45;

impl Worker {
    async fn single_update(self: Arc<Self>) -> Vec<(String, Status)> {
        let mut needs_crank = Vec::<Address>::new();
        let mut res = vec![];
        let factory = self.factory.read().factory;

        let (markets, status) = match get_markets(&self.cosmos, factory).await {
            Ok(markets) => {
                let status = Status::success(format!("Loaded markets: {markets:?}"), None);
                (markets, status)
            }
            Err(e) => (
                vec![],
                Status::error(format!("Unable to load markets: {e:?}")),
            ),
        };
        res.push(("load-markets".to_owned(), status));

        for market in markets {
            let status = match self.check_crank(&market).await {
                Ok(None) => {
                    self.epochs.log_inactive();
                    Status::success("No crank messages waiting", Some(TOO_OLD_SECONDS))
                }
                Ok(Some(work)) => {
                    self.epochs.log_active();
                    // For now, assume that if any message appears in the queue, we
                    // need to use all wallets to complete processing. Hopefully we
                    // can refine better in the future.
                    for _ in self.config.crank_wallets.iter() {
                        needs_crank.push(*market.market.get_address());
                    }
                    Status::success(
                        format!("Turning the crank, next work item is {work:?}"),
                        Some(TOO_OLD_SECONDS),
                    )
                }
                Err(e) => Status::error(format!("Error while querying crank stats: {e:?}")),
            };
            res.push((market.market_id.to_string(), status));
        }

        let mut handles = vec![];
        for (idx, wallet) in self.config.crank_wallets.iter().cloned().enumerate() {
            let market = needs_crank.pop();
            let cosmos = self.cosmos.clone();
            let epochs = self.epochs.clone();
            let crank_messages = self.crank_messages.clone();
            handles.push((
                format!("wallet-{idx}"),
                tokio::task::spawn(async move {
                    match market {
                        None => Status::success(
                            "No crank messages required for this wallet",
                            Some(TOO_OLD_SECONDS),
                        ),
                        Some(market) => {
                            let mk_txbuilder = |msg: &Vec<u8>| {
                                TxBuilder::default().add_message(MsgExecuteContract {
                                    sender: wallet.get_address_string(),
                                    contract: market.get_address_string(),
                                    msg: msg.clone(),
                                    funds: vec![],
                                })
                            };
                            let mut real_txbuilder = None;
                            for msg in &*crank_messages {
                                let txbuilder = mk_txbuilder(msg);
                                match txbuilder.simulate(&cosmos, &wallet).await {
                                    Err(e) => log::error!("Could not simulate crank: {e:?}"),
                                    Ok(res) => {
                                        if res.gas_used <= MAX_CRANK_GAS {
                                            real_txbuilder = Some(txbuilder);
                                            break;
                                        }
                                    }
                                }
                            }
                            let txbuilder = match real_txbuilder {
                                Some(txbuilder) => Ok(txbuilder),
                                None => match crank_messages.last() {
                                    None => Err(anyhow::anyhow!("No crank messages available")),
                                    Some(msg) => Ok(mk_txbuilder(msg)),
                                },
                            };
                            let res = match txbuilder {
                                Err(e) => Err(e),
                                Ok(txbuilder) => {
                                    txbuilder.sign_and_broadcast(&cosmos, &wallet).await
                                }
                            };
                            match res {
                                Ok(res) => {
                                    epochs.log_stats(&res);
                                    Status::success(
                                        format!(
                                    "Successfully turned crank with wallet {wallet}, txhash == {}",
                                    res.txhash
                                ),
                                        Some(TOO_OLD_SECONDS),
                                    )
                                }
                                Err(e) => Status::error(format!(
                                    "Unable to turn crank with wallet {wallet}: {e:?}"
                                )),
                            }
                        }
                    }
                }),
            ));
        }

        for (key, handle) in handles {
            let status = match handle.await {
                Ok(status) => status,
                Err(e) => Status::error(format!("Panic occurred while cranking: {e:?}")),
            };
            res.push((key, status));
        }

        res
    }

    async fn check_crank(&self, market: &Market) -> Result<Option<market::crank::CrankWorkInfo>> {
        let market::entry::StatusResp { next_crank, .. } = market
            .market
            .query(market::entry::QueryMsg::Status {})
            .await?;

        Ok(next_crank)
    }
}
