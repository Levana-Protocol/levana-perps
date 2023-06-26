use cosmwasm_std::Decimal256;
use msg::prelude::*;
use shared::storage::MarketId;

use crate::{instantiate::AddMarketParams, store_code::PYTH_BRIDGE};

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
        let instantiate_market = app.make_instantiate_market(self.market)?;
        let pyth_bridge = match app.pyth_info {
            Some(pyth_info) => {
                let code_id = app.tracker.require_code_by_type(&opt, PYTH_BRIDGE).await?;
                Some(
                    pyth_info
                        .make_pyth_bridge(code_id, &app.basic.wallet, &factory)
                        .await?,
                )
            }
            None => None,
        };
        let add_market_params = AddMarketParams {
            trading_competition: app.trading_competition,
            faucet_admin: Some(app.wallet_manager),
            price_admin: app.price_admin,
            factory,
            initial_borrow_fee_rate: self.initial_borrow_fee_rate,
            pyth_bridge,
        };
        instantiate_market
            .add(&app.basic.wallet, &app.basic.cosmos, add_market_params)
            .await?;
        Ok(())
    }
}
