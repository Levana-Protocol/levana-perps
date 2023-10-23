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
    prelude::MarketContract,
};
use shared::storage::{ErrorId, PerpError};

use crate::{mainnet::strip_nulls, spot_price_config::get_spot_price_config, util::add_cosmos_msg};
use cosmos::Address;

#[derive(clap::Parser)]
pub(super) struct TransferDaoFeesOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// The market to transfer fees from
    /// if none is supplied, then it will transfer from all markets
    #[clap(long)]
    market_id: Option<String>,
}

impl TransferDaoFeesOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    TransferDaoFeesOpts { factory, market_id }: TransferDaoFeesOpts,
) -> Result<()> {
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

    for market in markets {
        match market
            .market
            .execute(
                app.get_wallet()?,
                vec![],
                msg::contracts::market::entry::ExecuteMsg::TransferDaoFees {},
            )
            .await
        {
            Ok(tx) => {
                log::info!(
                    "Transferred fees from market {} to DAO: {}",
                    market.market_id,
                    tx.txhash
                );
            }
            Err(err) => {
                if err
                    .root_cause()
                    .to_string()
                    .contains("No DAO fees available to transfer")
                {
                    log::info!("No DAO fees available on {}", market.market_id);
                } else {
                    return Err(err);
                }
            }
        }
    }

    Ok(())
}
