use anyhow::Result;

use crate::cli::Opt;
use crate::factory::Factory;

#[derive(clap::Parser)]
pub(crate) struct EnableMarketOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
}

impl EnableMarketOpt {
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
                Factory::from_contract(app.basic.cosmos.make_contract(address.parse()?))
            }
        };
        let res = factory.enable_all(&app.basic.wallet).await?;
        log::info!("Enabled market in {}", res.txhash);

        log::info!("Don't forget to deposit liquidity into the contract!");
        Ok(())
    }
}
