use anyhow::{Context, Result};
use cosmos::HasAddress;
use cosmwasm_std::{to_binary, CosmosMsg, Empty, WasmMsg};
use msg::{
    contracts::market::{config::ConfigUpdate, entry::ExecuteOwnerMsg},
    prelude::MarketExecuteMsg,
};
use shared::storage::MarketId;

use crate::{cli::Opt, factory::Factory, mainnet::strip_nulls};

use super::MainnetFactories;

#[derive(clap::Parser)]
pub(super) struct UpdateConfigOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// Market ID, if omitted updates all markets
    #[clap(long)]
    market: Option<MarketId>,
    /// Update config JSON message
    #[clap(long)]
    config: String,
}

impl UpdateConfigOpts {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: Opt,
    UpdateConfigOpts {
        factory,
        market,
        config,
    }: UpdateConfigOpts,
) -> Result<()> {
    let update: ConfigUpdate = serde_json::from_str(&config).context("Invalid ConfigUpdate")?;
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;

    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let markets = match market {
        Some(market) => vec![factory.get_market(market).await?],
        None => factory.get_markets().await?,
    };

    let owner = factory.query_owner().await?;
    log::info!("CW3 contract: {owner}");

    let msgs = markets
        .into_iter()
        .map(|market| {
            anyhow::Ok(CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                contract_addr: market.market.get_address_string(),
                msg: to_binary(&strip_nulls(MarketExecuteMsg::Owner(
                    ExecuteOwnerMsg::ConfigUpdate {
                        update: update.clone(),
                    },
                ))?)?,
                funds: vec![],
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    log::info!("Message: {}", serde_json::to_string(&msgs)?);

    Ok(())
}
