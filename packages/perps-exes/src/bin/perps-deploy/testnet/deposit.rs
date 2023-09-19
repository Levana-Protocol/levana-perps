use anyhow::Result;
use msg::prelude::*;
use perps_exes::{contracts::Factory, prelude::MarketContract};

use crate::cli::Opt;

#[derive(clap::Parser)]
pub(crate) struct DepositOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
    /// Which market to deposit into
    #[clap(long)]
    market: MarketId,
    /// How much collateral to deposit?
    #[clap(long)]
    amount: NonZero<Collateral>,
}

impl DepositOpt {
    pub(crate) async fn go(self, opt: Opt) -> Result<()> {
        let app = opt.load_app(&self.family).await?;
        let factory = app
            .tracker
            .get_contract_by_family("factory", &self.family, None)
            .await?;
        let factory = match factory {
            msg::contracts::tracker::entry::ContractResp::NotFound {} => {
                anyhow::bail!("No factory found")
            }
            msg::contracts::tracker::entry::ContractResp::Found { address, .. } => {
                log::info!("Found factory address {address}");
                Factory::from_contract(app.basic.cosmos.make_contract(address.parse()?))
            }
        };
        let market = factory.get_market(self.market).await?;
        log::info!("Found market address {}", market.market);
        let market = MarketContract::new(market.market);
        let status = market.status().await?;
        let res = market
            .deposit(&app.basic.wallet, &status, self.amount)
            .await?;
        log::info!("Deposited collateral in {}", res.txhash);

        Ok(())
    }
}
