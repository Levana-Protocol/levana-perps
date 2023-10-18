use anyhow::Context;
use msg::contracts::market::config::ConfigUpdate;
use perps_exes::prelude::MarketContract;

use perps_exes::contracts::Factory;

#[derive(clap::Parser)]
pub(crate) struct UpdateMarketConfigsOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
    /// Update message JSON
    update: String,
}

impl UpdateMarketConfigsOpt {
    pub(crate) async fn go(self, opt: crate::cli::Opt) -> anyhow::Result<()> {
        let update = serde_json::from_str::<ConfigUpdate>(&self.update)
            .context("Invalid config update message")?;
        let app = opt.load_app(&self.family).await?;
        let wallet = app.basic.get_wallet()?;
        let factory = app
            .tracker
            .get_contract_by_family("factory", &self.family, None)
            .await?;
        let factory = match factory {
            msg::contracts::tracker::entry::ContractResp::NotFound {} => {
                anyhow::bail!("No factory found")
            }
            msg::contracts::tracker::entry::ContractResp::Found { address, .. } => {
                Factory::from_contract(app.basic.cosmos.make_contract(address.parse()?))
            }
        };

        let markets = factory.get_markets().await?;

        for market in markets {
            log::info!("Updating market: {}", market.market_id);
            let market_contract = MarketContract::new(market.market);
            let res = market_contract
                .config_update(wallet, update.clone())
                .await?;
            log::info!("Updated {} in {}", market.market_id, res.txhash);
        }

        Ok(())
    }
}
