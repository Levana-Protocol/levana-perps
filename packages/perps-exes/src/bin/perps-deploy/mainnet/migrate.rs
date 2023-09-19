use anyhow::Result;
use cosmos::HasAddress;
use cosmwasm_std::{to_binary, CosmosMsg, Empty, WasmMsg};
use msg::prelude::*;
use perps_exes::contracts::Factory;

use crate::cli::Opt;

use super::MainnetFactories;

#[derive(clap::Parser)]
pub(super) struct MigrateOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// New market code ID to use
    #[clap(long)]
    market_code_id: u64,
}

impl MigrateOpts {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: Opt,
    MigrateOpts {
        factory,
        market_code_id,
    }: MigrateOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;

    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let current_market_code_id = factory.query_market_code_id().await?;

    if current_market_code_id.get_code_id() != market_code_id {
        let msg = CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
            contract_addr: factory.get_address_string(),
            msg: to_binary(&FactoryExecuteMsg::SetMarketCodeId {
                code_id: market_code_id.to_string(),
            })?,
            funds: vec![],
        });
        log::info!("Update factory message");
        let owner = factory.query_owner().await?;
        log::info!("CW3 contract: {owner}");
        log::info!("Message: {}", serde_json::to_string(&msg)?);
    }

    let mut msgs = Vec::<CosmosMsg<Empty>>::new();
    let migration_admin = factory.query_migration_admin().await?;

    for market in factory.get_markets().await? {
        let market = market.market;
        let info = market.info().await?;
        anyhow::ensure!(info.admin == migration_admin.get_address_string(), "Invalid migration admin set up. Factory says: {migration_admin}. But market contract {market} has {}.", info.admin);
        if info.code_id != market_code_id {
            msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: market.get_address_string(),
                new_code_id: market_code_id,
                msg: to_binary(&serde_json::json!({}))?,
            }));
        }
    }

    if !msgs.is_empty() {
        log::info!("Migrate existing markets");
        log::info!("CW3 contract: {migration_admin}");
        log::info!("Message: {}", serde_json::to_string(&msgs)?);
    }

    Ok(())
}
