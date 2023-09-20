use chrono::{DateTime, Utc};
use msg::{contracts::tracker::entry::ContractResp, prelude::*};
use perps_exes::prelude::MarketContract;

use perps_exes::contracts::{Factory, MarketInfo};

#[derive(clap::Parser)]
pub(crate) struct DisableMarketAtOpt {
    /// Timestamp to disable at
    timestamp: DateTime<Utc>,
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
}

impl DisableMarketAtOpt {
    pub(crate) async fn go(&self, opt: crate::cli::Opt) -> Result<()> {
        let app = opt.load_app(&self.family).await?;
        let factory = app
            .tracker
            .get_contract_by_family("factory", &self.family, None)
            .await?;
        let factory = match factory {
            ContractResp::NotFound {} => anyhow::bail!("Could not find factory contract"),
            ContractResp::Found { address, .. } => app.basic.cosmos.make_contract(address.parse()?),
        };
        let factory = Factory::from_contract(factory);

        while self.timestamp > Utc::now() {
            let delta = self.timestamp - Utc::now();
            let to_sleep = delta.num_seconds().min(30);

            let delta = tokio::time::Duration::from_secs(delta.num_seconds().try_into()?);
            let to_sleep = tokio::time::Duration::from_secs(to_sleep.try_into()?);

            log::info!("Timestamp still in the future, sleeping for: {to_sleep:?}. Total wait time: {delta:?}.");
            tokio::time::sleep(to_sleep).await;
        }

        let markets = factory.get_markets().await?;
        for market in markets {
            log::info!("Shutting down trades in market {}", market.market_id);
            let res = factory
                .disable_trades(&app.basic.wallet, market.market_id)
                .await?;
            log::info!("Trades shut down in {}", res.txhash);
        }
        Ok(())
    }
}

#[derive(clap::Parser)]
pub(crate) struct CloseAllPositionsOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
}

impl CloseAllPositionsOpt {
    pub(crate) async fn go(&self, opt: crate::cli::Opt) -> Result<()> {
        let app = opt.load_app(&self.family).await?;
        let factory = app.tracker.get_factory(&self.family).await?;
        let markets = factory.get_markets().await?;

        for MarketInfo {
            market, market_id, ..
        } in markets
        {
            let market = MarketContract::new(market);
            log::info!("Closing all positions for {market_id}");
            let res = market.close_all_positions(&app.basic.wallet).await?;
            log::info!("Closed in {}", res.txhash);
        }
        Ok(())
    }
}
