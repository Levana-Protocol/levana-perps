use std::sync::Arc;

use cosmos::{
    proto::cosmos::base::abci::v1beta1::TxResponse, Address, Contract, HasAddress, Wallet,
};
use msg::{
    contracts::faucet::entry::{
        ExecuteMsg, FaucetAsset, MultitapRecipient, QueryMsg, TapEligibleResponse,
    },
    prelude::*,
};
use tokio::sync::mpsc::error::TrySendError;

use super::{App, AppBuilder};

impl AppBuilder {
    pub(super) fn launch_faucet_task(&mut self, runner: FaucetBotRunner) {
        let contract = self.app.cosmos.make_contract(self.app.config.faucet);
        self.watch_background(runner.start(contract));
    }
}

pub(crate) struct FaucetBot {
    wallet_address: Address,
    hcaptcha_secret: String,
    tx: tokio::sync::mpsc::Sender<TapRequest>,
}

impl FaucetBot {
    pub(crate) fn new(wallet: Wallet, hcaptcha_secret: String) -> (Self, FaucetBotRunner) {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let bot = FaucetBot {
            wallet_address: wallet.get_address(),
            hcaptcha_secret,
            tx,
        };
        let runner = FaucetBotRunner { wallet, rx };
        (bot, runner)
    }

    pub(crate) fn get_wallet_address(&self) -> Address {
        self.wallet_address
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
    ) -> Result<Arc<String>> {
        let contract = app.cosmos.make_contract(app.config.faucet);
        match contract
            .query(QueryMsg::IsTapEligible {
                addr: recipient.get_address_string().into(),
                assets: cw20s
                    .iter()
                    .map(|addr| FaucetAsset::Cw20(addr.get_address_string().into()))
                    .collect(),
            })
            .await?
        {
            TapEligibleResponse::Eligible {} => (),
            TapEligibleResponse::Ineligible {
                seconds: _,
                message,
            } => anyhow::bail!("{message}"),
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let req = TapRequest {
            recipient,
            cw20s,
            tx,
        };
        if let Err(e) = self.tx.try_send(req) {
            return Err(match e {
                TrySendError::Full(_) => {
                    anyhow::anyhow!("Too many faucet requests in the queue, please try again later")
                }
                TrySendError::Closed(_) => {
                    anyhow::anyhow!("Internal server error, faucet queue is already closed")
                }
            });
        }
        match rx.await {
            Ok(res) => res,
            Err(e) => Err(anyhow::anyhow!(
                "Internal server error, please try tapping again: {e}"
            )),
        }
    }
}

struct TapRequest {
    recipient: Address,
    cw20s: Vec<Address>,
    tx: tokio::sync::oneshot::Sender<Result<Arc<String>>>,
}

pub struct FaucetBotRunner {
    wallet: Wallet,
    rx: tokio::sync::mpsc::Receiver<TapRequest>,
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
                            log::warn!("Faucet tapper no longer waiting: {e:?}");
                        }
                    }
                    break;
                }
                Err(e) => {
                    log::error!("{e:?}");
                    retries += 1;
                    if retries >= 10 {
                        let msg = format!("Error occurred while transacting against the faucet contract, please try again later: {e:?}");
                        for req in reqs {
                            if let Err(e) = req.tx.send(Err(anyhow::anyhow!("{msg}"))) {
                                log::warn!("Faucet tapper no longer waiting: {e:?}");
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    async fn try_tap(&self, contract: &Contract, reqs: &[TapRequest]) -> Result<TxResponse> {
        contract
            .execute(
                &self.wallet,
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

// let faucet = app.cosmos.make_contract(app.config.faucet);
// let wallet = app.faucet_bot.wallet.write().await;
// let res = faucet
//     .execute(
//         &wallet,
//         vec![],
//         // FIXME move to multitap
//         ExecuteMsg::Tap {
//             assets: query
//                 .cw20s
//                 .into_iter()
//                 .map(|x| FaucetAsset::Cw20(x.get_address_string().into()))
//                 // This will end up as a no-op if the faucet gas a gas allowance set.
//                 .chain(std::iter::once(FaucetAsset::Native(
//                     app.cosmos.get_gas_coin().clone(),
//                 )))
//                 .collect(),
//             recipient: query.recipient.get_address_string().into(),
//             amount: None,
//         },
//     )
//     .await?;
