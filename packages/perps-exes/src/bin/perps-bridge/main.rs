#![deny(clippy::as_conversions)]

mod context;
mod handler;

use anyhow::{Context as AnyhowContext, Result};
use context::*;
use dotenv::dotenv;
use futures::{channel::mpsc::unbounded, future, pin_mut, StreamExt, TryStreamExt};
use log::info;
use multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    fs::OpenOptions,
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_util::task::LocalPoolHandle;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    dotenv().ok();
    let ctx = Context::new().await;
    init_logger(&ctx);

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&ctx.listen_addr()).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", ctx.listen_addr());

    let pool = LocalPoolHandle::new(5);

    while let Ok((stream, client_addr)) = listener.accept().await {
        pool.spawn_pinned({
            let ctx = ctx.clone();
            move || {
                let market = Arc::new(Mutex::new(
                    PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap(),
                ));
                async move {
                    accept_connection(ctx.clone(), market, stream, client_addr)
                        .await
                        .unwrap();
                }
            }
        });
    }

    Ok(())
}

async fn accept_connection(
    ctx: Arc<Context>,
    market: Arc<Mutex<PerpsMarket>>,
    stream: TcpStream,
    addr: SocketAddr,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .context("Error during the websocket handshake occurred")?;

    info!("New WebSocket connection: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    ctx.peer_map.lock().await.insert(addr, tx);

    // wipe the log
    if ctx.opts.log {
        let mut lock = ctx.log_file.lock().await;

        *lock = Some(
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&ctx.opts.log_file_path)
                .await
                .unwrap(),
        );
    }

    // get the websocket stream itself
    let (outgoing, incoming) = ws_stream.split();
    let broadcast_incoming = incoming.try_for_each(|msg| {
        let ctx = ctx.clone();
        let market = market.clone();
        async move {
            ctx.handle_msg(market, &addr, msg).await.unwrap();
            Ok(())
        }
    });

    // erm... magic
    let receive_from_others = rx.map(Ok).forward(outgoing);
    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    ctx.handle_disconnect(&addr).await;

    Ok(())
}

fn init_logger(ctx: &Context) {
    let env = env_logger::Env::default().default_filter_or(if ctx.opts.verbose {
        format!("{}=debug,cosmos=debug,info", env!("CARGO_CRATE_NAME"))
    } else {
        "info".to_owned()
    });
    env_logger::Builder::from_env(env).init();
}
