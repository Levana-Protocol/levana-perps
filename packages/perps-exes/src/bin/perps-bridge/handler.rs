use crate::{context::LogFlag, future::Future};
use anyhow::{Context as AnyhowContext, Result};
use bigdecimal::ToPrimitive;
use tokio::{io::AsyncWriteExt, sync::Mutex};
use tokio_tungstenite::tungstenite::Message;

use super::context::Context;
use cw_multi_test::AppResponse;
use msg::{
    bridge::{
        BridgeToClientMsg, BridgeToClientWrapper, ClientToBridgeMsg, ClientToBridgeWrapper,
        ExecError,
    },
    shared::prelude::*,
};
use multi_test::{market_wrapper::PerpsMarket, time::TimeJump};
use std::net::SocketAddr;
use std::sync::Arc;

impl Context {
    async fn handle_app_response(
        &self,
        client_addr: &SocketAddr,
        elapsed: f64,
        wrapper: &ClientToBridgeWrapper,
        resp: Result<AppResponse>,
    ) -> Result<()> {
        let send_msg = match resp {
            Ok(resp) => BridgeToClientMsg::MarketExecSuccess {
                events: resp.events,
            },
            Err(err) => match err.downcast_ref::<PerpError>() {
                Some(err) => {
                    BridgeToClientMsg::MarketExecFailure(ExecError::PerpError(err.clone()))
                }
                None => BridgeToClientMsg::MarketExecFailure(ExecError::Unknown(err.to_string())),
            },
        };

        self.send_to_peer(client_addr, elapsed, wrapper.msg_id, send_msg)
            .await?;
        Ok(())
    }

    async fn with_timing<A, F, FUT>(&self, f: F) -> (f64, A)
    where
        F: FnOnce() -> FUT,
        FUT: Future<Output = A>,
    {
        let start = std::time::Instant::now();
        let res = f().await;
        let elapsed = start.elapsed();
        let elapsed = elapsed.as_secs().to_f64().unwrap_or_default() + (elapsed.subsec_nanos().to_f64().unwrap_or_default() / 1_000_000_000.0);
        (elapsed, res)
    }

    pub async fn handle_msg(
        &self,
        market: Arc<Mutex<PerpsMarket>>,
        client_addr: &SocketAddr,
        msg: Message,
    ) -> Result<()> {
        if let Ok(text) = msg.to_text() {
            match serde_json::from_str::<ClientToBridgeWrapper>(text) {
                Ok(wrapper) => {
                    let should_log = match &wrapper.msg {
                        ClientToBridgeMsg::QueryMarket { .. } => {
                            self.opts.log_flags.contains(&LogFlag::QueryMarket)
                        }
                        ClientToBridgeMsg::ExecMarket { .. } => {
                            self.opts.log_flags.contains(&LogFlag::ExecMarket)
                        }
                        ClientToBridgeMsg::RefreshPrice => {
                            self.opts.log_flags.contains(&LogFlag::RefreshPrice)
                        }
                        ClientToBridgeMsg::Crank => self.opts.log_flags.contains(&LogFlag::Crank),
                        ClientToBridgeMsg::MintCollateral { .. } => {
                            self.opts.log_flags.contains(&LogFlag::MintCollateral)
                        }
                        ClientToBridgeMsg::MintAndDepositLp { .. } => {
                            self.opts.log_flags.contains(&LogFlag::MintAndDepositLp)
                        }
                        ClientToBridgeMsg::TimeJumpSeconds { .. } => {
                            self.opts.log_flags.contains(&LogFlag::TimeJumpSeconds)
                        }
                    };

                    if should_log {
                        self.log_msg(&wrapper).await?;
                    }

                    match &wrapper.msg {
                        ClientToBridgeMsg::MintCollateral { amount } => {
                            let (elapsed, resp) = self
                                .with_timing(|| async {
                                    market
                                        .lock()
                                        .await
                                        .exec_mint_tokens(&wrapper.user, amount.into_number())
                                })
                                .await;
                            self.handle_app_response(client_addr, elapsed, &wrapper, resp)
                                .await?;
                            Ok(())
                        }
                        ClientToBridgeMsg::MintAndDepositLp { amount } => {
                            let (elapsed, resp) = self
                                .with_timing(|| async {
                                    market.lock().await.exec_mint_and_deposit_liquidity(
                                        &wrapper.user,
                                        amount.into_number(),
                                    )
                                })
                                .await;
                            self.handle_app_response(client_addr, elapsed, &wrapper, resp)
                                .await?;
                            Ok(())
                        }

                        ClientToBridgeMsg::RefreshPrice => {
                            let (elapsed, resp) = self
                                .with_timing(|| async { market.lock().await.exec_refresh_price() })
                                .await;
                            self.handle_app_response(
                                client_addr,
                                elapsed,
                                &wrapper,
                                resp.map(|res| res.base),
                            )
                            .await?;
                            Ok(())
                        }

                        ClientToBridgeMsg::Crank => {
                            let (elapsed, resp) = self
                                .with_timing(|| async {
                                    market.lock().await.exec_crank(&wrapper.user)
                                })
                                .await;
                            self.handle_app_response(client_addr, elapsed, &wrapper, resp)
                                .await?;
                            Ok(())
                        }

                        ClientToBridgeMsg::ExecMarket { exec_msg, funds } => {
                            let (elapsed, resp) = match funds {
                                Some(funds) => {
                                    self.with_timing(|| async {
                                        market.lock().await.exec_funds(
                                            &wrapper.user,
                                            exec_msg,
                                            funds.into_number(),
                                        )
                                    })
                                    .await
                                }
                                None => {
                                    self.with_timing(|| async {
                                        market.lock().await.exec(&wrapper.user, exec_msg)
                                    })
                                    .await
                                }
                            };
                            self.handle_app_response(client_addr, elapsed, &wrapper, resp)
                                .await?;

                            Ok(())
                        }
                        ClientToBridgeMsg::QueryMarket { query_msg } => {
                            let (elapsed, resp) = self
                                .with_timing(|| async { market.lock().await.raw_query(query_msg) })
                                .await;
                            let resp = resp?;
                            self.send_to_peer(
                                client_addr,
                                elapsed,
                                wrapper.msg_id,
                                BridgeToClientMsg::MarketQueryResult { result: resp },
                            )
                            .await?;
                            Ok(())
                        }
                        ClientToBridgeMsg::TimeJumpSeconds { seconds } => {
                            let (elapsed, resp) = self
                                .with_timing(|| async {
                                    market.lock().await.set_time(TimeJump::Seconds(*seconds))
                                })
                                .await;
                            resp?;
                            self.send_to_peer(
                                client_addr,
                                elapsed,
                                wrapper.msg_id,
                                BridgeToClientMsg::TimeJumpResult {},
                            )
                            .await?;
                            Ok(())
                        }
                    }
                }
                Err(e) => {
                    log::warn!("unable to parse message: {}", e);
                    Ok(())
                }
            }
        } else {
            log::info!("got a non-text message");
            Ok(())
        }
    }

    pub async fn handle_disconnect(&self, addr: &SocketAddr) {
        self.peer_map.lock().await.remove(addr);
    }

    async fn send_to_peer(
        &self,
        client_addr: &SocketAddr,
        elapsed: f64,
        id: u64,
        msg: BridgeToClientMsg,
    ) -> Result<()> {
        let msg = serde_json::to_string(&BridgeToClientWrapper {
            msg_id: id,
            elapsed,
            msg,
        })?;
        let msg = Message::Text(msg);

        self.peer_map
            .lock()
            .await
            .get(client_addr)
            .context(format!("no such peer at {}", client_addr))?
            .unbounded_send(msg)
            .context(format!("unable to send message to {}", client_addr))?;

        Ok(())
    }

    async fn log_msg(&self, wrapper: &ClientToBridgeWrapper) -> Result<()> {
        if let Some(log_file) = self.log_file.lock().await.as_mut() {
            let text = serde_json::to_string(wrapper).unwrap();
            log_file.write_all(text.as_bytes()).await.unwrap();
            log_file.write_u8(b'\n').await?;
        }

        Ok(())
    }
}
