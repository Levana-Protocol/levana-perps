use anyhow::{Context, Result};
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::{to_json_binary, CosmosMsg, Empty, WasmMsg};
use perps_exes::contracts::Factory;
use perpswap::storage::MarketId;
use perpswap::{namespace::CLOSE_ALL_POSITIONS, prelude::MarketExecuteMsg};

use crate::{cli::Opt, util::add_cosmos_msg};

use super::MainnetFactories;

#[derive(clap::Parser)]
pub(super) struct CloseAllPositionsOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// Market ID
    #[clap(long)]
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
    tracing::info!("CW3 contract: {wind_down}");

    let mut builder = TxBuilder::default();
    let mut msgs = vec![];
    let market = if market.is_empty() {
        let mut markets = vec![];
        for market in factory.get_markets().await? {
            let res = market.market.query_raw(CLOSE_ALL_POSITIONS).await?;
            if res.is_empty() {
                markets.push(market);
            } else {
                tracing::info!(
                    "Skipping close-all-positions on already closed market {}",
                    market.market_id
                );
            }
        }
        markets
    } else {
        let mut market2 = vec![];
        for market in market {
            market2.push(factory.get_market(market).await?)
        }
        market2
    };
    for market in market {
        let market = market.market;
        let msg = CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
            contract_addr: market.get_address_string(),
            funds: vec![],
            msg: to_json_binary(&MarketExecuteMsg::CloseAllPositions {})?,
        });
        add_cosmos_msg(&mut builder, wind_down, &msg)?;
        msgs.push(msg);
    }

    tracing::info!("Message: {}", serde_json::to_string(&msgs)?);

    let res = builder
        .simulate(&app.cosmos, &[wind_down])
        .await
        .context("Error while simulating")?;
    tracing::info!("Successfully simulated messages");
    tracing::debug!("Simulate response: {res:?}");

    Ok(())
}
