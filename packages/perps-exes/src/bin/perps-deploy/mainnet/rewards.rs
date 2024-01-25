use std::path::PathBuf;

use anyhow::{Context, Result};
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Empty, WasmMsg};
use msg::{
    contracts::market::{
        config::{Config, ConfigUpdate},
        entry::ExecuteOwnerMsg,
    },
    prelude::MarketExecuteMsg,
};
use perps_exes::{
    config::{ChainConfig, MainnetFactories, MarketConfigUpdates, PriceConfig},
    contracts::{Factory, MarketInfo},
    prelude::{Collateral, MarketContract},
};
use shared::storage::{ErrorId, PerpError};

use crate::{mainnet::strip_nulls, spot_price_config::get_spot_price_config, util::add_cosmos_msg};
use cosmos::Address;

#[derive(clap::Parser)]
pub(super) struct RewardsOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// The market to collect rewards from
    /// if none is supplied, then it will transfer from all markets
    #[clap(long)]
    market_id: Option<String>,
}

impl RewardsOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(opt: crate::cli::Opt, RewardsOpts { factory, market_id }: RewardsOpts) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let mut markets = factory.get_markets().await?;
    if let Some(market_id) = market_id {
        markets = markets
            .into_iter()
            .filter(|m| m.market_id.as_str() == market_id)
            .collect();
    }

    let wallet = app.get_wallet()?;
    let mut to_collect = vec![];

    for market in markets {
        let market_id = market.market_id;
        let market = MarketContract::new(market.market);
        let lp_info = market.lp_info(wallet).await?;

        if lp_info.available_yield == Collateral::zero() {
            log::info!("{market_id}: No yield available");
        } else {
            log::info!("{market_id}: Want to collect {}", lp_info.available_yield);
            to_collect.push((market_id, market.get_address()));
        }
    }

    for chunk in to_collect.chunks(5) {
        log::info!("Going to collect for markets: {chunk:?}");
        let mut tx = TxBuilder::default();
        for (_, addr) in chunk {
            tx.add_execute_message(addr, wallet, vec![], MarketExecuteMsg::ClaimYield {})?;
        }
        let res = tx.sign_and_broadcast(&app.cosmos, wallet).await?;
        log::info!("Collected in: {}", res.txhash);
    }

    Ok(())
}
