use std::sync::Arc;

use cosmos::{proto::cosmos::base::abci::v1beta1::TxResponse, Address, Contract, HasAddress};
use perpswap::{
    contracts::faucet::entry::{
        ExecuteMsg, FaucetAsset, IneligibleReason, MultitapRecipient, QueryMsg, TapEligibleResponse,
    },
    prelude::*,
};
use tokio::sync::mpsc::error::TrySendError;

use super::{App, AppBuilder, WalletPool};

impl AppBuilder {
    pub(super) fn launch_faucet_task(&mut self, runner: FaucetBotRunner) {
        let contract = self.app.cosmos.make_contract(runner.faucet);
        self.watch_background(runner.start(contract));
    }
}

pub(crate) struct FaucetBot {
    hcaptcha_secret: String,
    tx: tokio::sync::mpsc::Sender<TapRequest>,
    faucet: Address,
}

impl FaucetBot {
    pub(crate) fn new(
        hcaptcha_secret: String,
        faucet: Address,
        pool: WalletPool,
    ) -> (Self, FaucetBotRunner) {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let bot = FaucetBot {
            hcaptcha_secret,
            tx,
            faucet,
        };
        let runner = FaucetBotRunner { rx, faucet, pool };
        (bot, runner)
    }

    pub(crate) fn get_hcaptcha_secret(&self) -> &String {
        &self.hcaptcha_secret
    }

    /// Returns the transaction hash on success
    pub(crate) async fn tap(
        &self,
        app: &App,
        recipient: Address,
        cw20s: Vec<Address>,
    ) -> Result<Arc<String>, FaucetTapError> {
        let contract = app.cosmos.make_contract(self.faucet);
        match contract
            .query(QueryMsg::IsTapEligible {
                addr: recipient.get_address_string().into(),
                assets: cw20s
                    .iter()
                    .map(|addr| FaucetAsset::Cw20(addr.get_address_string().into()))
                    .collect(),
            })
            .await
            .map_err(|e| FaucetTapError::QueryEligibility {
                inner: format!("{e:?}"),
            })? {
            TapEligibleResponse::Eligible {} => (),
            TapEligibleResponse::Ineligible {
                seconds,
                reason,
                message,
            } => {
                return Err(FaucetTapError::Ineligible {
                    seconds,
                    message,
                    reason,
                })
            }
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let req = TapRequest {
            recipient,
            cw20s,
            tx,
        };
        if let Err(e) = self.tx.try_send(req) {
            return Err(match &e {
                TrySendError::Full(_) => FaucetTapError::TooManyRequests {},
                TrySendError::Closed(_) => FaucetTapError::ClosedChannel {
                    is_oneshot: false,
                    receive: false,
                },
            });
        }
        match rx.await {
            Ok(res) => res,
            Err(_) => Err(FaucetTapError::ClosedChannel {
                is_oneshot: true,
                receive: true,
            }),
        }
    }
}

struct TapRequest {
    recipient: Address,
    cw20s: Vec<Address>,
    tx: tokio::sync::oneshot::Sender<Result<Arc<String>, FaucetTapError>>,
}

#[derive(serde::Serialize, thiserror::Error, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FaucetTapError {
    #[error("Unable to query tap eligibility from chain.")]
    QueryEligibility { inner: String },
    #[error("Internal server error. A channel was closed prematurely.")]
    ClosedChannel { is_oneshot: bool, receive: bool },
    #[error("Too many faucet requests in the queue, please try again later.")]
    TooManyRequests {},
    #[error("Wallet is ineligible to tap the faucet: {message}")]
    Ineligible {
        seconds: Decimal256,
        message: String,
        reason: IneligibleReason,
    },
    #[error("The faucet server was unable to execute against the faucet smart contract.")]
    Contract { inner: String },
    #[error("The faucet server was unable to query the captcha service. Please try again later.")]
    CannotQueryCaptcha {},
    #[error("The captcha provided was invalid, please try again.")]
    InvalidCaptcha {},
    #[error("Unfortunately we cannot provide a faucet for mainnet.")]
    Mainnet {},
}

pub(crate) struct FaucetBotRunner {
    rx: tokio::sync::mpsc::Receiver<TapRequest>,
    faucet: Address,
    pool: WalletPool,
}

impl FaucetBotRunner {
    async fn start(mut self, contract: Contract) -> Result<()> {
        loop {
            self.single(&contract).await;
        }
    }

    async fn single(&mut self, contract: &Contract) {
        let mut reqs = vec![self
            .rx
            .recv()
            .await
            .expect("Impossible! FaucetBot rx is closed")];

        let mut retries = 0;
        loop {
            // Get up to 10 requests to process at a time
            //
            // Do this inside the loop so during retries we pick up additional requests.
            while reqs.len() < 10 {
                match self.rx.try_recv() {
                    Ok(req) => reqs.push(req),
                    // No more requests waiting
                    Err(_) => break,
                }
            }

            match self.try_tap(contract, &reqs).await {
                Ok(res) => {
                    let txhash = Arc::new(res.txhash);
                    for req in reqs {
                        if let Err(e) = req.tx.send(Ok(txhash.clone())) {
                            tracing::warn!("Faucet tapper no longer waiting: {e:?}");
                        }
                    }
                    break;
                }
                Err(e) => {
                    tracing::error!("{e:?}");
                    retries += 1;
                    if retries >= 10 {
                        for req in reqs {
                            if let Err(e) = req.tx.send(Err(FaucetTapError::Contract {
                                inner: e.to_string(),
                            })) {
                                tracing::warn!("Faucet tapper no longer waiting: {e:?}");
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    async fn try_tap(
        &self,
        contract: &Contract,
        reqs: &[TapRequest],
    ) -> cosmos::Result<TxResponse> {
        contract
            .execute(
                &*self.pool.get().await,
                vec![],
                ExecuteMsg::Multitap {
                    recipients: reqs.iter().map(|x| x.into()).collect(),
                },
            )
            .await
    }
}

impl From<&TapRequest> for MultitapRecipient {
    fn from(
        TapRequest {
            recipient,
            cw20s,
            tx: _,
        }: &TapRequest,
    ) -> Self {
        MultitapRecipient {
            addr: recipient.get_address_string().into(),
            assets: cw20s
                .iter()
                .map(|addr| FaucetAsset::Cw20(addr.get_address_string().into()))
                .collect(),
        }
    }
}
