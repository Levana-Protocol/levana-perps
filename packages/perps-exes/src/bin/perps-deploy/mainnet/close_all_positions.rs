use anyhow::{Context, Result};
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::{to_binary, CosmosMsg, Empty, WasmMsg};
use msg::prelude::MarketExecuteMsg;
use perps_exes::contracts::Factory;
use shared::storage::MarketId;

use crate::{cli::Opt, util::add_cosmos_msg};

use super::MainnetFactories;

#[derive(clap::Parser)]
pub(super) struct CloseAllPositionsOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// Market ID
    #[clap(long, required = true)]
    market: Vec<MarketId>,
}

impl CloseAllPositionsOpts {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: Opt,
    CloseAllPositionsOpts { factory, market }: CloseAllPositionsOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;

    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let wind_down = factory.query_wind_down().await?;
    log::info!("CW3 contract: {wind_down}");

    let mut builder = TxBuilder::default();
    let mut msgs = vec![];
    for market in market {
        let market = factory.get_market(market).await?.market;
        let msg = CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
            contract_addr: market.get_address_string(),
            funds: vec![],
            msg: to_binary(&MarketExecuteMsg::CloseAllPositions {})?,
        });
        add_cosmos_msg(&mut builder, wind_down, &msg)?;
        msgs.push(msg);
    }

    log::info!("Message: {}", serde_json::to_string(&msgs)?);

    let res = builder
        .simulate(&app.cosmos, &[wind_down])
        .await
        .context("Error while simulating")?;
    log::info!("Successfully simulated messages");
    log::debug!("Simulate response: {res:?}");

    Ok(())
}
