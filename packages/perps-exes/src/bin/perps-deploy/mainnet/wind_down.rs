use anyhow::{Context, Result};
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::{to_json_binary, CosmosMsg, Empty, WasmMsg};
use perpswap::{
    prelude::FactoryExecuteMsg,
    shutdown::{ShutdownEffect, ShutdownImpact},
};
use perps_exes::contracts::Factory;
use perpswap::storage::MarketId;

use crate::{cli::Opt, util::add_cosmos_msg};

use super::MainnetFactories;

#[derive(clap::Parser)]
pub(super) struct WindDownOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// Market ID
    #[clap(long)]
    market: Vec<MarketId>,
    /// Which parts of the market to shut down
    #[clap(long, required = true)]
    impacts: Vec<String>,
    /// Enable instead of disable
    #[clap(long)]
    enable: bool,
    /// Use the kill switch wallet instead
    #[clap(long)]
    kill_switch: bool,
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
        kill_switch,
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

    let market = if market.is_empty() {
        factory
            .get_markets()
            .await?
            .into_iter()
            .map(|x| x.market_id)
            .collect()
    } else {
        market
    };

    let wind_down = if kill_switch {
        factory.query_kill_switch().await?
    } else {
        factory.query_wind_down().await?
    };
    tracing::info!("CW3 contract: {wind_down}");

    let msg = CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
        contract_addr: factory.get_address_string(),
        funds: vec![],
        msg: to_json_binary(&FactoryExecuteMsg::Shutdown {
            impacts: impacts.clone(),
            markets: market,
            effect: if enable {
                ShutdownEffect::Enable
            } else {
                ShutdownEffect::Disable
            },
        })?,
    });
    tracing::info!("Message: {}", serde_json::to_string(&msg)?);

    let mut builder = TxBuilder::default();
    add_cosmos_msg(&mut builder, wind_down, &msg)?;
    let res = builder
        .simulate(&app.cosmos, &[wind_down])
        .await
        .context("Error while simulating")?;
    tracing::info!("Successfully simulated messages");
    tracing::debug!("Simulate response: {res:?}");

    Ok(())
}
