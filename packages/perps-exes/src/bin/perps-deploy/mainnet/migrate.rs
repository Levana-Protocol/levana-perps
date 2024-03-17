use anyhow::Result;
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::{to_binary, CosmosMsg, Empty, WasmMsg};
use msg::prelude::*;
use perps_exes::contracts::{ConfiguredCodeIds, Factory};

use crate::{cli::Opt, util::add_cosmos_msg};

use super::MainnetFactories;

#[derive(clap::Parser)]
pub(super) struct MigrateOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    #[clap(long)]
    market_code_id: Option<u64>,
    #[clap(long)]
    factory_code_id: Option<u64>,
    #[clap(long)]
    liquidity_token_code_id: Option<u64>,
    #[clap(long)]
    position_token_code_id: Option<u64>,
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
        factory_code_id,
        liquidity_token_code_id,
        position_token_code_id,
    }: MigrateOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;
    let app = opt.load_app_mainnet(factory.network).await?;

    let factory = app.cosmos.make_contract(factory.address);
    let current_factory_code_id = factory.info().await?.code_id;
    let factory = Factory::from_contract(factory);

    let ConfiguredCodeIds {
        market: current_market_code_id,
        position_token: current_position_token_code_id,
        liquidity_token: current_liquidity_token_code_id,
    } = factory.query_code_ids().await?;

    let mut factory_msgs = vec![];
    if let Some(market_code_id) = market_code_id {
        if current_market_code_id.get_code_id() != market_code_id {
            factory_msgs.push(CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                contract_addr: factory.get_address_string(),
                msg: to_binary(&FactoryExecuteMsg::SetMarketCodeId {
                    code_id: market_code_id.to_string(),
                })?,
                funds: vec![],
            }));
        }
    }
    if let Some(liquidity_token_code_id) = liquidity_token_code_id {
        if current_liquidity_token_code_id.get_code_id() != liquidity_token_code_id {
            factory_msgs.push(CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                contract_addr: factory.get_address_string(),
                msg: to_binary(&FactoryExecuteMsg::SetLiquidityTokenCodeId {
                    code_id: liquidity_token_code_id.to_string(),
                })?,
                funds: vec![],
            }));
        }
    }
    if let Some(position_token_code_id) = position_token_code_id {
        if current_position_token_code_id.get_code_id() != position_token_code_id {
            factory_msgs.push(CosmosMsg::<Empty>::Wasm(WasmMsg::Execute {
                contract_addr: factory.get_address_string(),
                msg: to_binary(&FactoryExecuteMsg::SetPositionTokenCodeId {
                    code_id: position_token_code_id.to_string(),
                })?,
                funds: vec![],
            }));
        }
    }

    let mut builder = TxBuilder::default();
    let mut signers = vec![];

    if !factory_msgs.is_empty() {
        log::info!("Update factory message");
        let owner = factory.query_owner().await?;
        log::info!("CW3 contract: {owner}");
        log::info!("Message: {}", serde_json::to_string(&factory_msgs)?);
        signers.push(owner);
        for msg in &factory_msgs {
            add_cosmos_msg(&mut builder, owner, msg)?;
        }
    }

    let mut msgs = Vec::<CosmosMsg<Empty>>::new();
    let migration_admin = factory.query_migration_admin().await?;

    if let Some(factory_code_id) = factory_code_id {
        if current_factory_code_id != factory_code_id {
            msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                contract_addr: factory.get_address_string(),
                new_code_id: factory_code_id,
                msg: to_binary(&msg::contracts::factory::entry::MigrateMsg {})?,
            }));
        }
    }

    for market in factory.get_markets().await? {
        let lp = market.liquidity_token_lp;
        let xlp = market.liquidity_token_xlp;
        let pos = market.position_token;
        let market = market.market;
        let info = market.info().await?;
        anyhow::ensure!(info.admin == migration_admin.get_address_string(), "Invalid migration admin set up. Factory says: {migration_admin}. But market contract {market} has {}.", info.admin);
        if let Some(market_code_id) = market_code_id {
            if info.code_id != market_code_id {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                    contract_addr: market.get_address_string(),
                    new_code_id: market_code_id,
                    msg: to_binary(&msg::contracts::market::entry::MigrateMsg {})?,
                }));
            }
        }

        if let Some(liquidity_token_code_id) = liquidity_token_code_id {
            let info = lp.info().await?;
            if info.code_id != liquidity_token_code_id {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                    contract_addr: lp.get_address_string(),
                    new_code_id: liquidity_token_code_id,
                    msg: to_binary(&msg::contracts::liquidity_token::entry::MigrateMsg {})?,
                }));
            }
            let info = xlp.info().await?;
            if info.code_id != liquidity_token_code_id {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                    contract_addr: xlp.get_address_string(),
                    new_code_id: liquidity_token_code_id,
                    msg: to_binary(&msg::contracts::liquidity_token::entry::MigrateMsg {})?,
                }));
            }
        }

        let info = pos.info().await?;
        if let Some(position_token_code_id) = position_token_code_id {
            if info.code_id != position_token_code_id {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                    contract_addr: pos.get_address_string(),
                    new_code_id: position_token_code_id,
                    msg: to_binary(&msg::contracts::position_token::entry::MigrateMsg {})?,
                }));
            }
        }
    }

    if !msgs.is_empty() {
        log::info!("Migrate existing markets");
        log::info!("CW3 contract: {migration_admin}");
        log::info!("Message: {}", serde_json::to_string(&msgs)?);
        signers.push(migration_admin);
        for msg in &msgs {
            add_cosmos_msg(&mut builder, migration_admin, msg)?;
        }
    }

    if signers.is_empty() {
        log::info!("No messages generated");
    } else {
        let res = builder
            .simulate(&app.cosmos, &signers)
            .await
            .context("Unable to simulate CW3 messages")?;
        log::info!("Successfully simulated messages");
        log::debug!("Full simulate response: {res:?}");
    }

    Ok(())
}
