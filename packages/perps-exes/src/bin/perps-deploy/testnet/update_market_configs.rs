use anyhow::Context;
use perps_exes::prelude::MarketContract;
use perpswap::contracts::market::config::ConfigUpdate;

use perps_exes::contracts::Factory;
use perpswap::storage::MarketId;

#[derive(clap::Parser)]
pub(crate) struct UpdateMarketConfigsOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
    /// Update only this specific market
    #[clap(long)]
    market_id: Option<MarketId>,
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
            perpswap::contracts::tracker::entry::ContractResp::NotFound {} => {
                anyhow::bail!("No factory found")
            }
            perpswap::contracts::tracker::entry::ContractResp::Found { address, .. } => {
                Factory::from_contract(app.basic.cosmos.make_contract(address.parse()?))
            }
        };

        let mut markets = factory.get_markets().await?;

        if let Some(market_id) = self.market_id {
            let market = markets
                .into_iter()
                .find(|market| market.market_id == market_id)
                .context(format!("No market id {market_id} found"))?;
            markets = vec![market];
        }

        for market in markets {
            tracing::info!("Updating market: {}", market.market_id);
            let market_contract = MarketContract::new(market.market);
            let res = market_contract
                .config_update(wallet, update.clone())
                .await?;
            tracing::info!("Updated {} in {}", market.market_id, res.txhash);
        }

        Ok(())
    }
}
