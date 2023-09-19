use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

use crate::{config::CONFIG, prelude::*};
use anyhow::{anyhow, Result};
use awsm_web::loaders::helpers::{spawn_handle, FutureHandle};
use cosmwasm_std::{from_binary, Addr};
use dominator_helpers::futures::AsyncLoader;
use futures::lock::Mutex as AsyncMutex;
use futures::{
    channel::oneshot,
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use gloo_net::websocket::{futures::WebSocket, Message};
use msg::{
    bridge::{
        BridgeToClientMsg, BridgeToClientWrapper, ClientToBridgeMsg, ClientToBridgeWrapper,
        ExecError,
    },
    contracts::market::entry::{ExecuteMsg, QueryMsg},
};
use serde::{de::DeserializeOwned, Deserialize};
use wasm_bindgen_futures::spawn_local;

pub struct BridgeResponse<T> {
    pub msg_id: u64,
    pub msg_elapsed: f64,
    pub data: T,
}

pub struct Bridge {
    pub write_sink: AsyncMutex<SplitSink<WebSocket, Message>>,
    pub read_handle: FutureHandle,
    pub(self) id_counter: AtomicU64,
    pub send_loader: AsyncLoader,
    pub reply_map: Rc<RefCell<HashMap<u64, oneshot::Sender<BridgeToClientWrapper>>>>,
}

impl Bridge {
    pub async fn connect() -> Result<Rc<Self>> {
        let ws = WebSocket::open(CONFIG.bridge_addr)?;
        let (mut write_sink, mut read) = ws.split();
        let reply_map = Rc::new(RefCell::new(HashMap::new()));

        let read_handle = spawn_handle(clone!(reply_map => async move {
            while let Some(msg) = read.next().await {
                let wrapper = match msg {
                    Ok(msg) => match msg {
                        Message::Text(msg) => serde_json::from_str::<BridgeToClientWrapper>(&msg).map_err(|err| err.into()),
                        _ => Err(anyhow!("binary not supported yet"))
                    },
                    Err(err) => Err(err.into())
                };

                match wrapper {
                    Ok(wrapper) => {
                        let tx:oneshot::Sender<BridgeToClientWrapper> = reply_map.borrow_mut().remove(&wrapper.msg_id).unwrap();
                        tx.send(wrapper).unwrap();
                    },
                    Err(err) => {
                        log::error!("error parsing message: {}", err);
                    }
                }


            }
        }));

        Ok(Rc::new(Self {
            write_sink: AsyncMutex::new(write_sink),
            read_handle,
            id_counter: AtomicU64::new(0),
            send_loader: AsyncLoader::new(),
            reply_map,
        }))
    }

    pub async fn query_market<T: DeserializeOwned>(
        &self,
        msg: QueryMsg,
    ) -> Result<BridgeResponse<T>> {
        let send_msg = ClientToBridgeMsg::QueryMarket { query_msg: msg };
        let (msg_id, recv_wrapper) = self.send_msg(send_msg).await?;
        match recv_wrapper.msg {
            BridgeToClientMsg::MarketQueryResult { result } => Ok(BridgeResponse {
                msg_id,
                msg_elapsed: recv_wrapper.elapsed,
                data: from_binary(&result)?,
            }),
            _ => Err(anyhow!("unexpected response")),
        }
    }

    pub async fn refresh_price(&self) -> Result<BridgeResponse<Vec<cosmwasm_std::Event>>> {
        self.send_exec_msg(ClientToBridgeMsg::RefreshPrice).await
    }

    pub async fn crank(&self) -> Result<BridgeResponse<Vec<cosmwasm_std::Event>>> {
        self.send_exec_msg(ClientToBridgeMsg::Crank).await
    }

    pub async fn mint_collateral(
        &self,
        amount: NumberGtZero,
    ) -> Result<BridgeResponse<Vec<cosmwasm_std::Event>>> {
        self.send_exec_msg(ClientToBridgeMsg::MintCollateral { amount: amount })
            .await
    }
    pub async fn mint_and_deposit_lp(
        &self,
        amount: NumberGtZero,
    ) -> Result<BridgeResponse<Vec<cosmwasm_std::Event>>> {
        self.send_exec_msg(ClientToBridgeMsg::MintAndDepositLp { amount })
            .await
    }
    pub async fn exec_market(
        &self,
        msg: ExecuteMsg,
        funds: Option<NumberGtZero>,
    ) -> Result<BridgeResponse<Vec<cosmwasm_std::Event>>> {
        self.send_exec_msg(ClientToBridgeMsg::ExecMarket {
            exec_msg: msg,
            funds,
        })
        .await
    }
    pub async fn time_jump(&self, seconds: i64) -> Result<BridgeResponse<i64>> {
        let send_msg = ClientToBridgeMsg::TimeJumpSeconds { seconds };
        let (msg_id, recv_wrapper) = self.send_msg(send_msg).await?;
        match recv_wrapper.msg {
            BridgeToClientMsg::TimeJumpResult {} => Ok(BridgeResponse {
                msg_id,
                msg_elapsed: recv_wrapper.elapsed,
                data: seconds,
            }),
            _ => Err(anyhow!("unexpected response")),
        }
    }

    async fn send_exec_msg(
        &self,
        send_msg: ClientToBridgeMsg,
    ) -> Result<BridgeResponse<Vec<cosmwasm_std::Event>>> {
        let (msg_id, recv_wrapper) = self.send_msg(send_msg).await?;
        match recv_wrapper.msg {
            BridgeToClientMsg::MarketExecSuccess { events } => Ok(BridgeResponse {
                msg_id,
                msg_elapsed: recv_wrapper.elapsed,
                data: events,
            }),
            BridgeToClientMsg::MarketExecFailure(err) => match err {
                ExecError::PerpError(err) => Err(err.into()),
                ExecError::Unknown(err) => Err(anyhow!(err)),
            },
            _ => Err(anyhow!("unexpected response")),
        }
    }

    async fn send_msg(&self, msg: ClientToBridgeMsg) -> Result<(u64, BridgeToClientWrapper)> {
        let msg_id = self
            .id_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.reply_map.borrow_mut().insert(msg_id, tx);

        let msg = ClientToBridgeWrapper {
            msg_id,
            user: match &msg {
                ClientToBridgeMsg::ExecMarket { exec_msg, .. } => match exec_msg {
                    ExecuteMsg::Owner(_) => CONFIG.protocol_owner_addr.clone(),
                    _ => CONFIG.user_addr.clone(),
                },
                _ => CONFIG.user_addr.clone(),
            },
            msg,
        };

        let msg = serde_json::to_string(&msg)?;

        self.write_sink
            .lock()
            .await
            .send(Message::Text(msg))
            .await
            .unwrap();

        let recv_wrapper = rx.await.map_err(|_| anyhow::anyhow!("channel closed"))?;
        Ok((msg_id, recv_wrapper))
    }
}
