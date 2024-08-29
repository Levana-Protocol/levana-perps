use anyhow::Result;
use cosmos::{HasAddress, TxBuilder};
use msg::prelude::MarketExecuteMsg;
use perps_exes::{
    config::MainnetFactories,
    contracts::Factory,
    prelude::{Collateral, MarketContract},
};

#[derive(clap::Parser)]
pub(super) struct RewardsOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// The market to collect rewards from
    /// if none is supplied, then it will transfer from all markets
    #[clap(long)]
    market_id: Option<String>,
}

impl RewardsOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(opt: crate::cli::Opt, RewardsOpts { factory, market_id }: RewardsOpts) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let mut markets = factory.get_markets().await?;
    if let Some(market_id) = market_id {
        markets.retain(|m| m.market_id.as_str() == market_id);
    }

    let wallet = app.get_wallet()?;
    let mut to_collect = vec![];

    for market in markets {
        let market_id = market.market_id;
        let market = MarketContract::new(market.market);
        let lp_info = market.lp_info(wallet).await?;

        if lp_info.available_yield == Collateral::zero() {
            tracing::info!("{market_id}: No yield available");
        } else {
            tracing::info!("{market_id}: Want to collect {}", lp_info.available_yield);
            to_collect.push((market_id, market.get_address()));
        }
    }

    for chunk in to_collect.chunks(5) {
        tracing::info!("Going to collect for markets: {chunk:?}");
        let mut tx = TxBuilder::default();
        for (_, addr) in chunk {
            tx.add_execute_message(addr, wallet, vec![], MarketExecuteMsg::ClaimYield {})?;
        }
        let res = tx.sign_and_broadcast(&app.cosmos, wallet).await?;
        tracing::info!("Collected in: {}", res.txhash);
    }

    Ok(())
}
