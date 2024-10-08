use anyhow::{Context, Result};
use cosmos::HasAddress;
use cosmwasm_std::{to_json_binary, CosmosMsg, Empty, WasmMsg};
use msg::{
    contracts::market::{config::ConfigUpdate, entry::ExecuteOwnerMsg},
    prelude::MarketExecuteMsg,
};
use perps_exes::{
    config::{ChainConfig, PriceConfig},
    contracts::Factory,
};
use perpswap::storage::MarketId;

use crate::{cli::Opt, mainnet::strip_nulls, spot_price_config::get_spot_price_config};

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
    /// Add in the spot price config
    #[clap(long)]
    spot_price: bool,
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
        spot_price,
    }: UpdateConfigOpts,
) -> Result<()> {
    let update: ConfigUpdate = serde_json::from_str(&config).context("Invalid ConfigUpdate")?;
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;

    let spot_price_helper = if spot_price {
        Some({
            let chain_config = ChainConfig::load(factory.network)?;
            let price_config = PriceConfig::load()?;
            let oracle = opt.get_oracle_info(&chain_config, &price_config, factory.network)?;
            move |market_id| get_spot_price_config(&oracle, &market_id)
        })
    } else {
        None
    };

    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let markets = match market {
        Some(market) => vec![factory.get_market(market).await?],
        None => factory.get_markets().await?,
    };

    let owner = factory.query_owner().await?;
    tracing::info!("CW3 contract: {owner}");

    let msgs = markets
        .into_iter()
        .map(|market| {
            let mut update = update.clone();
            if let Some(helper) = &spot_price_helper {
                update.spot_price = Some(helper(market.market_id)?);
            }
            anyhow::Ok(CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                contract_addr: market.market.get_address_string(),
                msg: to_json_binary(&strip_nulls(MarketExecuteMsg::Owner(
                    ExecuteOwnerMsg::ConfigUpdate {
                        update: Box::new(update),
                    },
                ))?)?,
                funds: vec![],
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    tracing::info!("Message: {}", serde_json::to_string(&msgs)?);

    Ok(())
}
