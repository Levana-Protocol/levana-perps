use msg::prelude::*;
use perps_exes::contracts::Factory;
use shared::storage::MarketId;

use crate::instantiate::AddMarketParams;

#[derive(clap::Parser)]
pub(crate) struct AddMarketOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
    /// Which market to deposit into
    #[clap(long)]
    market: MarketId,
}

impl AddMarketOpt {
    pub(crate) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        let app = opt.load_app(&self.family).await?;
        let wallet = app.basic.get_wallet()?;
        let factory = app.tracker.get_factory(&self.family).await?.into_contract();

        let factory = Factory::from_contract(factory);
        let instantiate_market = app.make_instantiate_market(self.market.clone())?;

        let add_market_params = AddMarketParams {
            trading_competition: app.trading_competition,
            faucet_admin: Some(app.wallet_manager),
            factory,
        };
        instantiate_market
            .add(
                wallet,
                &app.basic.cosmos,
                &app.config_testnet,
                add_market_params,
            )
            .await?;
        Ok(())
    }
}
