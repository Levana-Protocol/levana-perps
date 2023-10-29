use std::path::PathBuf;

use anyhow::{Context, Result};
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::{to_binary, CosmosMsg, Empty, WasmMsg};
use msg::{
    contracts::market::{config::ConfigUpdate, entry::ExecuteOwnerMsg},
    prelude::{FactoryExecuteMsg, MarketExecuteMsg},
    shutdown::{ShutdownEffect, ShutdownImpact},
};
use perps_exes::{
    config::{ChainConfig, PriceConfig},
    contracts::Factory,
};
use shared::storage::MarketId;

use crate::{
    cli::Opt, mainnet::strip_nulls, spot_price_config::get_spot_price_config, util::add_cosmos_msg,
};

use super::MainnetFactories;

#[derive(clap::Parser)]
pub(super) struct WindDownOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// Market ID
    #[clap(long, required = true)]
    market: Vec<MarketId>,
    /// Which parts of the market to shut down
    #[clap(long, required = true)]
    impacts: Vec<String>,
    /// Enable instead of disable
    #[clap(long)]
    enable: bool,
}

impl WindDownOpts {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: Opt,
    WindDownOpts {
        factory,
        market,
        impacts,
        enable,
    }: WindDownOpts,
) -> Result<()> {
    let impacts = impacts
        .into_iter()
        .map(|impact| serde_json::from_value(serde_json::Value::String(impact)))
        .collect::<Result<Vec<ShutdownImpact>, _>>()?;

    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;

    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let wind_down = factory.query_wind_down().await?;
    log::info!("CW3 contract: {wind_down}");

    let msg = CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
        contract_addr: factory.get_address_string(),
        funds: vec![],
        msg: to_binary(&FactoryExecuteMsg::Shutdown {
            impacts: impacts.clone(),
            markets: market,
            effect: if enable {
                ShutdownEffect::Enable
            } else {
                ShutdownEffect::Disable
            },
        })?,
    });
    log::info!("Message: {}", serde_json::to_string(&msg)?);

    let mut builder = TxBuilder::default();
    add_cosmos_msg(&mut builder, wind_down, &msg)?;
    let res = builder
        .simulate(&app.cosmos, &[wind_down])
        .await
        .context("Error while simulating")?;
    log::info!("Successfully simulated messages");
    log::debug!("Simulate response: {res:?}");

    Ok(())
}
