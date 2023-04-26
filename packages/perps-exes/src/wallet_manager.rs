//! Generates new wallets and manages minting of CW20s

use std::{
    fmt::Debug,
    sync::{atomic::AtomicU32, Arc},
};

use anyhow::{Context, Result};
use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Address, AddressType, Cosmos, HasAddress,
    SeedPhrase, TxBuilder, Wallet,
};
use msg::{
    contracts::{cw20::Cw20Coin, market::entry::StatusResp},
    prelude::{Collateral, UnsignedDecimal},
};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct WalletManager {
    inner: Arc<Inner>,
}

struct Inner {
    seed: SeedPhrase,
    address_type: AddressType,
    next_index: AtomicU32,
    send_request: mpsc::Sender<MintRequest>,
}

struct MintRequest {
    addr: Address,
    amount: u128,
    cw20: Address,
    on_ready: oneshot::Sender<Result<()>>,
    faucet: Address,
    cosmos: Cosmos,
}

impl Debug for MintRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MintRequest")
            .field("addr", &self.addr)
            .field("amount", &self.amount)
            .field("cw20", &self.cw20)
            .field("faucet", &self.faucet)
            .finish()
    }
}

impl WalletManager {
    pub fn new(seed: SeedPhrase, address_type: AddressType) -> Result<Self> {
        let minter = seed.derive_cosmos_numbered(0)?.for_chain(address_type);
        log::info!("Wallet manager minter wallet: {minter}");
        let (send_request, recv_request) = mpsc::channel(100);
        let manager = WalletManager {
            inner: Arc::new(Inner {
                seed,
                address_type,
                next_index: AtomicU32::new(1),
                send_request,
            }),
        };
        tokio::task::spawn(background(recv_request, minter));
        Ok(manager)
    }

    pub fn get_wallet(&self, desc: impl AsRef<str>) -> Result<Wallet> {
        let idx = self
            .inner
            .next_index
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let wallet = self
            .inner
            .seed
            .derive_cosmos_numbered(idx)?
            .for_chain(self.inner.address_type);
        log::info!(
            "Got fresh wallet from manager for {}: {wallet}",
            desc.as_ref()
        );
        Ok(wallet)
    }

    pub async fn mint(
        &self,
        cosmos: Cosmos,
        addr: Address,
        amount: Collateral,
        status: &StatusResp,
        cw20: Address,
        faucet: Address,
    ) -> Result<()> {
        let (on_ready, wait_for_it) = oneshot::channel();
        self.inner
            .send_request
            .send(MintRequest {
                addr,
                amount: status
                    .collateral
                    .into_u128(amount.into_decimal256())?
                    .context("mint: got a None from into_u128")?,
                cw20,
                on_ready,
                faucet,
                cosmos,
            })
            .await?;
        wait_for_it.await?
    }
}

async fn background(mut recv_request: mpsc::Receiver<MintRequest>, minter: Wallet) {
    loop {
        let req = match recv_request.recv().await {
            None => return,
            Some(req) => req,
        };
        let cosmos = req.cosmos.clone();
        let mut requests = vec![req];

        while requests.len() <= 10 {
            match recv_request.try_recv() {
                Ok(req) => requests.push(req),
                Err(_) => break,
            }
        }

        let res = process_requests(&cosmos, &minter, &requests).await;
        if let Err(e) = &res {
            log::error!("Wallet manager: error processing requests: {e:?}");
        }

        for req in requests {
            if let Err(e) = req.on_ready.send(match &res {
                Ok(()) => Ok(()),
                Err(e) => Err(anyhow::anyhow!("{e:?}")),
            }) {
                log::error!("Error sending on_ready: {e:?}");
            }
        }
    }
}

async fn process_requests(
    cosmos: &Cosmos,
    minter: &Wallet,
    requests: &[MintRequest],
) -> Result<()> {
    let mut tx_builder = TxBuilder::default();
    for req in requests {
        tx_builder.add_message_mut(MsgExecuteContract {
            sender: minter.get_address_string(),
            contract: req.faucet.get_address_string(),
            msg: serde_json::to_vec(&msg::contracts::faucet::entry::ExecuteMsg::OwnerMsg(
                msg::contracts::faucet::entry::OwnerMsg::Mint {
                    cw20: req.cw20.get_address_string(),
                    balances: vec![Cw20Coin {
                        address: req.addr.get_address_string(),
                        amount: req.amount.into(),
                    }],
                },
            ))?,
            funds: vec![],
        });
    }
    tx_builder.sign_and_broadcast(cosmos, minter).await?;
    Ok(())
}
