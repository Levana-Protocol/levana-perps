use anyhow::{Context, Result};
use cosmos::proto::cosmos::base::abci::v1beta1::TxResponse;
use cosmos::proto::cosmos::tx::v1beta1::SimulateResponse;
use cosmos::proto::tendermint::abci::Event;
use perps_exes::{config::MainnetFactories, contracts::Factory};
use perpswap::storage::MarketExecuteMsg;

#[derive(clap::Parser)]
pub(super) struct ForceWithdrawAllOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// Restrict the sweep to a single market ID
    #[clap(long)]
    market_id: Option<String>,
    /// Maximum number of LP providers or limit orders to process per transaction
    #[clap(long)]
    limit: Option<u32>,
    /// Safety cap on transactions per market
    #[clap(long, default_value = "1000")]
    max_rounds_per_market: u32,
}

impl ForceWithdrawAllOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    ForceWithdrawAllOpts {
        factory,
        market_id,
        limit,
        max_rounds_per_market,
    }: ForceWithdrawAllOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));
    let wallet = app.get_wallet()?;

    let mut markets = factory.get_markets().await?;
    if let Some(market_id) = market_id {
        markets.retain(|market| market.market_id.as_str() == market_id);
        anyhow::ensure!(!markets.is_empty(), "No matching market found");
    }

    for market in markets {
        tracing::info!("Force withdrawing all funds in {}", market.market_id);
        let simulated = market
            .market
            .simulate(
                wallet,
                vec![],
                MarketExecuteMsg::ForceWithdrawAll { limit },
                None,
            )
            .await
            .with_context(|| {
                format!(
                    "ForceWithdrawAll simulation failed for {}",
                    market.market_id
                )
            })?;
        let simulated_progress =
            force_withdraw_progress_from_simulation(&simulated).with_context(|| {
                format!(
                    "No force-withdraw-all event found in simulation for {}",
                    market.market_id
                )
            })?;

        tracing::info!(
            "{} simulation: processed_lp={}, processed_limit_orders={}, remaining_lp={}, remaining_limit_orders={}",
            market.market_id,
            simulated_progress.processed_lp,
            simulated_progress.processed_limit_orders,
            simulated_progress.remaining_lp,
            simulated_progress.remaining_limit_orders,
        );

        if simulated_progress.is_complete_without_work() {
            tracing::info!("Force withdrawal already complete for {}", market.market_id);
            continue;
        }

        anyhow::ensure!(
            simulated_progress.made_progress(),
            "ForceWithdrawAll simulation made no progress on {} but reported remaining work",
            market.market_id
        );

        for round in 1..=max_rounds_per_market {
            let tx = market
                .market
                .execute(wallet, vec![], MarketExecuteMsg::ForceWithdrawAll { limit })
                .await
                .with_context(|| format!("ForceWithdrawAll failed for {}", market.market_id))?;
            let progress = force_withdraw_progress(&tx)
                .with_context(|| format!("No force-withdraw-all event found in {}", tx.txhash))?;

            tracing::info!(
                "{} round {round}: tx={}, processed_lp={}, processed_limit_orders={}, remaining_lp={}, remaining_limit_orders={}",
                market.market_id,
                tx.txhash,
                progress.processed_lp,
                progress.processed_limit_orders,
                progress.remaining_lp,
                progress.remaining_limit_orders,
            );

            if !progress.remaining_lp && !progress.remaining_limit_orders {
                tracing::info!("Force withdrawal complete for {}", market.market_id);
                break;
            }

            anyhow::ensure!(
                progress.processed_lp > 0 || progress.processed_limit_orders > 0,
                "ForceWithdrawAll made no progress on {} but reported remaining work",
                market.market_id
            );

            anyhow::ensure!(
                round < max_rounds_per_market,
                "ForceWithdrawAll exceeded --max-rounds-per-market for {}",
                market.market_id
            );
        }
    }

    Ok(())
}

#[derive(Debug)]
struct ForceWithdrawProgress {
    processed_lp: u32,
    processed_limit_orders: u32,
    remaining_lp: bool,
    remaining_limit_orders: bool,
}

impl ForceWithdrawProgress {
    fn made_progress(&self) -> bool {
        self.processed_lp > 0 || self.processed_limit_orders > 0
    }

    fn is_complete_without_work(&self) -> bool {
        !self.made_progress() && !self.remaining_lp && !self.remaining_limit_orders
    }
}

fn force_withdraw_progress(tx: &TxResponse) -> Result<ForceWithdrawProgress> {
    force_withdraw_progress_from_events(tx.events.iter())
}

fn force_withdraw_progress_from_simulation(
    simulation: &SimulateResponse,
) -> Result<ForceWithdrawProgress> {
    force_withdraw_progress_from_events(
        simulation
            .result
            .as_ref()
            .context("simulation missing result")?
            .events
            .iter(),
    )
}

fn force_withdraw_progress_from_events<'a>(
    events: impl Iterator<Item = &'a Event>,
) -> Result<ForceWithdrawProgress> {
    let event = events
        .into_iter()
        .find(|event| event.r#type == "wasm-force-withdraw-all")
        .context("missing wasm-force-withdraw-all event")?;

    Ok(ForceWithdrawProgress {
        processed_lp: attr(event, "processed-lp")?.parse()?,
        processed_limit_orders: attr(event, "processed-limit-orders")?.parse()?,
        remaining_lp: attr(event, "remaining-lp")?.parse()?,
        remaining_limit_orders: attr(event, "remaining-limit-orders")?.parse()?,
    })
}

fn attr<'a>(event: &'a Event, key: &str) -> Result<&'a str> {
    event
        .attributes
        .iter()
        .find(|attr| attr.key == key)
        .map(|attr| attr.value.as_str())
        .with_context(|| format!("missing attribute {key}"))
}
