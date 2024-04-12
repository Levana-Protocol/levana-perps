use std::{collections::HashMap, ops::Add, path::PathBuf, sync::Arc};

use anyhow::Result;
use cosmos::Address;
use cosmwasm_std::OverflowError;
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
    /// Feeds paid so far
    #[clap(long)]
    paid_fees: Option<Usd>,
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
        paid_fees,
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

    struct FeeStats {
        trading: Usd,
        borrow: Usd,
        crank: Usd,
    }

    impl Add for FeeStats {
        type Output = anyhow::Result<Self, OverflowError>;

        fn add(mut self, rhs: Self) -> Self::Output {
            self.trading = (self.trading + rhs.trading)?;
            self.borrow = (self.borrow + rhs.borrow)?;
            self.crank = (self.crank + rhs.crank)?;

            Ok(self)
        }
    }

    impl FeeStats {
        pub(crate) fn new() -> Self {
            FeeStats {
                trading: Usd::zero(),
                borrow: Usd::zero(),
                crank: Usd::zero(),
            }
        }
    }

    let mut set = JoinSet::new();
    for _ in 0..workers {
        let csv = csv.clone();
        let rx = rx.clone();
        set.spawn(async move {
            let mut fees = FeeStats::new();
            loop {
                let (market_id, market, wallet) = match rx.recv().await {
                    Ok(tuple) => tuple,
                    Err(_) => break anyhow::Ok(fees),
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
                    fees = (fees
                        + FeeStats {
                            trading: pos.trading_fee_usd,
                            borrow: pos.borrow_fee_usd,
                            crank: pos.crank_fee_usd,
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
                    fees = (fees
                        + FeeStats {
                            trading: pos.trading_fee_usd,
                            borrow: pos.borrow_fee_usd,
                            crank: pos.crank_fee_usd,
                        })?;
                    csv.flush()?;
                }
            }
        });
    }

    std::mem::drop(rx);

    let mut stats = FeeStats::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(fees)) => stats = (stats + fees)?,
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

    log::info!("Total Trading USD: {}", stats.trading);
    log::info!("Total Borrow USD: {}", stats.borrow);
    log::info!("Crank Crank USD: {}", stats.crank);

    let total_fees = ((stats.trading + stats.borrow)? + stats.crank)?;
    log::info!("Total fees: {total_fees}");
    if let Some(paid_fees) = paid_fees {
        let to_pay = (total_fees - paid_fees)?;
        log::info!("Fees to be paid: {to_pay}");
    }

    Ok(())
}
