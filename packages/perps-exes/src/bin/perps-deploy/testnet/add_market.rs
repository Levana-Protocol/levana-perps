use std::path::PathBuf;

use cosmos::HasAddress;
use cosmwasm_std::Decimal256;
use msg::prelude::*;
use perps_exes::{
    config::{ChainConfig, PriceConfig},
    contracts::Factory,
};
use shared::storage::MarketId;

use crate::{
    app::PriceSourceConfig, instantiate::AddMarketParams, spot_price_config::get_spot_price_config,
};

#[derive(clap::Parser)]
pub(crate) struct AddMarketOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
    /// Which market to deposit into
    #[clap(long)]
    market: MarketId,
    /// Initial borrow fee rate
    #[clap(long, default_value = "0.2")]
    pub(crate) initial_borrow_fee_rate: Decimal256,
}

impl AddMarketOpt {
    pub(crate) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        let app = opt.load_app(&self.family).await?;
        let factory = app.tracker.get_factory(&self.family).await?.into_contract();

        let chain_config = ChainConfig::load(None::<PathBuf>, app.basic.network)?;
        let price_config = PriceConfig::load(None::<PathBuf>)?;
        let oracle = opt.get_oracle_info(&chain_config, &price_config, app.basic.network)?;

        let factory = Factory::from_contract(factory);
        let instantiate_market = app.make_instantiate_market(self.market.clone())?;

        let add_market_params = AddMarketParams {
            trading_competition: app.trading_competition,
            faucet_admin: Some(app.wallet_manager),
            factory,
            initial_borrow_fee_rate: self.initial_borrow_fee_rate,
            spot_price: get_spot_price_config(&oracle, &price_config, &self.market)?,
        };
        instantiate_market
            .add(
                &app.basic.wallet,
                &app.basic.cosmos,
                &app.config_testnet,
                add_market_params,
            )
            .await?;
        Ok(())
    }
}
