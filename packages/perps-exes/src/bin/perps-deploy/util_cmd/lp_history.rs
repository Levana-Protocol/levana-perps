use std::{collections::HashSet, path::PathBuf, sync::Arc};

use crate::cli::Opt;
use cosmos::{Address, CosmosNetwork};
use msg::{
    contracts::{cw20::entry::AllAccountsResponse, liquidity_token::LiquidityTokenKind},
    prelude::*,
};
use perps_exes::{contracts::Factory, prelude::MarketContract};
use tokio::{sync::Mutex, task::JoinSet};

#[derive(clap::Parser)]
pub(super) struct LpActionCsvOpt {
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: CosmosNetwork,
    /// Factory address
    #[clap(long)]
    factory: Address,
    /// Output CSV file
    #[clap(long)]
    csv: PathBuf,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: usize,
}

impl LpActionCsvOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(self, opt).await
    }
}

struct ToProcess {
    lp: Address,
    market: MarketContract,
    market_id: Arc<MarketId>,
}

async fn go(
    LpActionCsvOpt {
        network,
        factory,
        csv,
        workers,
    }: LpActionCsvOpt,
    opt: crate::cli::Opt,
) -> Result<()> {
    let cosmos = opt.connect(network).await?;
    let factory = Factory::from_contract(cosmos.make_contract(factory));
    let csv = ::csv::Writer::from_path(&csv)?;
    let csv = Arc::new(Mutex::new(csv));

    let mut set = JoinSet::<Result<()>>::new();
    let (tx, rx) = async_channel::bounded::<ToProcess>(workers * 4);

    let markets = factory.get_markets().await?;
    for market in markets {
        set.spawn(handle_market(market, tx.clone()));
    }

    // Without this drop, we get a deadlock below. Reason: as long as the send
    // side of the channel stays open, receiving on the other end of the channel
    // will block. When we drop all send sides, the next call to receive will
    // error out, which we use to detect that there is no more work available.
    std::mem::drop(tx);

    for _ in 0..workers {
        let csv = csv.clone();
        set.spawn(worker(rx.clone(), csv));
    }

    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(())) => (),
            Ok(Err(e)) => {
                set.abort_all();
                return Err(e);
            }
            Err(e) => {
                set.abort_all();
                return Err(e).context("Unexpected panic");
            }
        }
    }

    Ok(())
}

async fn handle_market(
    market: perps_exes::contracts::MarketInfo,
    tx: async_channel::Sender<ToProcess>,
) -> Result<()> {
    let mut seen = HashSet::new();
    let market_id = Arc::new(market.market_id);

    for kind in [LiquidityTokenKind::Lp, LiquidityTokenKind::Xlp] {
        let mut start_after = None;

        loop {
            let AllAccountsResponse { accounts } = market
                .market
                .query(MarketQueryMsg::LiquidityTokenProxy {
                    kind,
                    msg: msg::contracts::liquidity_token::entry::QueryMsg::AllAccounts {
                        start_after: start_after.take(),
                        limit: None,
                    },
                })
                .await?;
            if accounts.is_empty() {
                break;
            }
            for account in accounts {
                let addr: Address = account.as_str().parse()?;
                start_after = Some(account.into());
                let existed = seen.insert(addr);
                if !existed {
                    tx.send(ToProcess {
                        lp: addr,
                        market: MarketContract::new(market.market.clone()),
                        market_id: market_id.clone(),
                    })
                    .await?;
                }
            }
        }
    }

    Ok(())
}

async fn worker(
    rx: async_channel::Receiver<ToProcess>,
    csv: Arc<Mutex<csv::Writer<std::fs::File>>>,
) -> Result<()> {
    while let Ok(ToProcess {
        lp,
        market,
        market_id,
    }) = rx.recv().await
    {
        #[derive(serde::Serialize)]
        struct Record<'a> {
            market_id: &'a MarketId,
            addr: Address,
            yield_withdrawn: Collateral,
            yield_pending: Collateral,
            lp_collateral: Collateral,
            xlp_collateral: Collateral,
            xlp_unstaking: Collateral,
        }

        let lp_info = market.lp_info(lp).await?;

        let record = Record {
            market_id: &market_id,
            addr: lp,
            yield_withdrawn: lp_info.history.r#yield,
            yield_pending: lp_info.available_yield_lp + lp_info.available_yield_xlp,
            lp_collateral: lp_info.lp_collateral,
            xlp_collateral: lp_info.xlp_collateral,
            xlp_unstaking: lp_info
                .unstaking
                .map_or_else(Collateral::zero, |unstaking| {
                    Collateral::from_decimal256(
                        (unstaking.xlp_unstaking.raw() - unstaking.collected).into_decimal256()
                            / unstaking.xlp_unstaking.into_decimal256()
                            * unstaking.xlp_unstaking_collateral.into_decimal256(),
                    )
                }),
        };

        let mut csv = csv.lock().await;
        csv.serialize(&record)?;
        csv.flush()?;
    }

    Ok(())
}
