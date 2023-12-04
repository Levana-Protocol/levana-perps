//! Generates new wallets and manages minting of CW20s

use std::{
    fmt::{Debug, Display},
    sync::Arc,
};

use anyhow::{Context, Result};
use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Address, AddressHrp, Cosmos, HasAddress,
    SeedPhrase, TxBuilder, Wallet,
};
use msg::{
    contracts::{cw20::Cw20Coin, market::entry::StatusResp},
    prelude::{Collateral, UnsignedDecimal},
};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub(crate) struct WalletManager {
    inner: Arc<Inner>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize)]
pub(crate) enum ManagedWallet {
    Balance,
    Liquidity,
    Utilization,
    Trader(u32),
}

impl ManagedWallet {
    fn get_index(self) -> u32 {
        match self {
            ManagedWallet::Balance => 1,
            ManagedWallet::Liquidity => 2,
            ManagedWallet::Utilization => 3,
            ManagedWallet::Trader(x) => x + 3,
        }
    }
}

impl Display for ManagedWallet {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ManagedWallet::Balance => write!(f, "balance"),
            ManagedWallet::Liquidity => write!(f, "liquidity"),
            ManagedWallet::Utilization => write!(f, "utlization"),
            ManagedWallet::Trader(x) => write!(f, "trader-{x}"),
        }
    }
}

struct Inner {
    seed: SeedPhrase,
    address_type: AddressHrp,
    send_request: mpsc::Sender<MintRequest>,
    minter_address: Address,
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
    pub(crate) fn new(seed: SeedPhrase, address_type: AddressHrp) -> Result<Self> {
        let minter = seed
            .clone()
            .with_cosmos_numbered(0)
            .with_hrp(address_type)?;
        tracing::info!("Wallet manager minter wallet: {minter}");
        let (send_request, recv_request) = mpsc::channel(100);
        let manager = WalletManager {
            inner: Arc::new(Inner {
                seed,
                address_type,
                send_request,
                minter_address: minter.get_address(),
            }),
        };
        tokio::task::spawn(background(recv_request, minter));
        Ok(manager)
    }

    pub(crate) fn get_wallet(&self, desc: ManagedWallet) -> Result<Wallet> {
        let idx = desc.get_index();
        let wallet = self
            .inner
            .seed
            .clone()
            .with_cosmos_numbered(idx.into())
            .with_hrp(self.inner.address_type)?;
        tracing::info!("Got fresh wallet from manager for {desc}: {wallet}",);
        Ok(wallet)
    }

    pub(crate) async fn mint(
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

    /// Get the address of the minter itself.
    pub(crate) fn get_minter_address(&self) -> Address {
        self.inner.minter_address
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
            tracing::error!("Wallet manager: error processing requests: {e:?}");
        }

        for req in requests {
            if let Err(e) = req.on_ready.send(match &res {
                Ok(()) => Ok(()),
                Err(e) => Err(anyhow::anyhow!("{e:?}")),
            }) {
                tracing::error!("Error sending on_ready: {e:?}");
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
        tx_builder.add_message(MsgExecuteContract {
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
    tx_builder
        .sign_and_broadcast(cosmos, minter)
        .await
        .with_context(|| format!("Error processing token requests with minter {minter}"))?;
    Ok(())
}
