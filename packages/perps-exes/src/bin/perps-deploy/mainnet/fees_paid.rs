use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::Result;
use cosmos::Address;
use msg::contracts::market::position::PositionId;
use parking_lot::Mutex;
use perps_exes::{
    config::MainnetFactories,
    contracts::{Factory, MarketInfo},
    prelude::{MarketContract, Signed, Usd},
};
use shared::storage::MarketId;
use tokio::task::JoinSet;

#[derive(clap::Parser)]
pub(super) struct FeesPaidOpts {
    /// The wallet that paid the fees
    #[clap(long)]
    wallet: Vec<Address>,
    /// Destination file
    #[clap(long)]
    csv: PathBuf,
    /// Number of concurrent tasks
    #[clap(long, default_value_t = 16)]
    workers: u32,
}
impl FeesPaidOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    FeesPaidOpts {
        wallet,
        csv,
        workers,
    }: FeesPaidOpts,
) -> Result<()> {
    let csv = ::csv::Writer::from_path(&csv)?;
    let csv = Arc::new(Mutex::new(csv));
    #[derive(serde::Serialize)]
    struct Record<'a> {
        wallet: Address,
        market: &'a MarketId,
        position: PositionId,
        status: &'static str,
        trading_usd: Usd,
        borrow_usd: Usd,
        funding_usd: Signed<Usd>,
        dnf_usd: Signed<Usd>,
        crank_usd: Usd,
    }

    let factories = MainnetFactories::load()?;

    let mut wallets = HashMap::<&'static str, Vec<Address>>::new();

    for wallet in wallet {
        let factory = match wallet.hrp().as_str() {
            "osmo" => "osmomainnet1",
            "sei" => "seimainnet1",
            "inj" => "injmainnet1",
            hrp => anyhow::bail!("Unsupported address type: {hrp}"),
        };
        wallets.entry(factory).or_default().push(wallet);
    }

    let rx = {
        let (tx, rx) = async_channel::unbounded();

        for (factory, wallets) in wallets {
            let factory = factories.get(factory)?;

            let app = opt.load_app_mainnet(factory.network).await?;
            let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));
            let markets = factory.get_markets().await?;
            for MarketInfo {
                market_id, market, ..
            } in markets
            {
                let market_id = Arc::new(market_id);
                for wallet in &wallets {
                    tx.send((
                        market_id.clone(),
                        MarketContract::new(market.clone()),
                        *wallet,
                    ))
                    .await?;
                }
            }
        }

        rx
    };

    let mut set = JoinSet::new();
    for _ in 0..workers {
        let csv = csv.clone();
        let rx = rx.clone();
        set.spawn(async move {
            loop {
                let (market_id, market, wallet) = match rx.recv().await {
                    Ok(tuple) => tuple,
                    Err(_) => break anyhow::Ok(()),
                };
                log::info!("Processing {market_id}/{wallet}");
                for pos in market.all_open_positions(wallet).await?.info {
                    let mut csv = csv.lock();
                    csv.serialize(&Record {
                        wallet,
                        market: &market_id,
                        position: pos.id,
                        status: "open",
                        trading_usd: pos.trading_fee_usd,
                        borrow_usd: pos.borrow_fee_usd,
                        funding_usd: pos.funding_fee_usd,
                        dnf_usd: pos.delta_neutrality_fee_usd,
                        crank_usd: pos.crank_fee_usd,
                    })?;
                    csv.flush()?;
                }
                for pos in market.all_closed_positions(wallet).await? {
                    let mut csv = csv.lock();
                    csv.serialize(&Record {
                        wallet,
                        market: &market_id,
                        position: pos.id,
                        status: "closed",
                        trading_usd: pos.trading_fee_usd,
                        borrow_usd: pos.borrow_fee_usd,
                        funding_usd: pos.funding_fee_usd,
                        dnf_usd: pos.delta_neutrality_fee_usd,
                        crank_usd: pos.crank_fee_usd,
                    })?;
                    csv.flush()?;
                }
            }
        });
    }

    std::mem::drop(rx);

    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(())) => (),
            Ok(Err(e)) => {
                set.abort_all();
                return Err(e);
            }
            Err(e) => {
                set.abort_all();
                return Err(e.into());
            }
        }
    }

    Ok(())
}
