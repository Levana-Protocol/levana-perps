use anyhow::Result;
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::{to_json_binary, CosmosMsg, Empty, WasmMsg};
use perps_exes::contracts::{ConfiguredCodeIds, Factory};
use perpswap::prelude::*;

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
    #[clap(long)]
    markets: Vec<MarketId>,
    #[clap(long)]
    wrapped: bool,
    #[clap(long, default_value = "Add real title")]
    title: String,
    #[clap(long, default_value = "Add real description")]
    desc: String,
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
        markets,
        wrapped,
        title,
        desc,
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
                msg: to_json_binary(&FactoryExecuteMsg::SetMarketCodeId {
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
                msg: to_json_binary(&FactoryExecuteMsg::SetLiquidityTokenCodeId {
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
                msg: to_json_binary(&FactoryExecuteMsg::SetPositionTokenCodeId {
                    code_id: position_token_code_id.to_string(),
                })?,
                funds: vec![],
            }));
        }
    }

    let mut builder = TxBuilder::default();
    let mut signers = vec![];

    if !factory_msgs.is_empty() {
        tracing::info!("Update factory message");
        let owner = factory
            .query_owner()
            .await?
            .context("The factory owner is not provided")?;
        tracing::info!("CW3 contract: {owner}");
        tracing::info!(
            "Message: {}",
            message_string(&factory_msgs, wrapped, &title, &desc)?
        );
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
                msg: to_json_binary(&perpswap::contracts::factory::entry::MigrateMsg {})?,
            }));
        }
    }

    for market in factory.get_markets().await? {
        if !markets.is_empty() && !markets.contains(&market.market_id) {
            tracing::info!("Skipping market: {}", market.market_id);
            continue;
        }
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
                    msg: to_json_binary(&perpswap::contracts::market::entry::MigrateMsg {})?,
                }));
            }
        }

        if let Some(liquidity_token_code_id) = liquidity_token_code_id {
            let info = lp.info().await?;
            if info.code_id != liquidity_token_code_id {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                    contract_addr: lp.get_address_string(),
                    new_code_id: liquidity_token_code_id,
                    msg: to_json_binary(
                        &perpswap::contracts::liquidity_token::entry::MigrateMsg {},
                    )?,
                }));
            }
            let info = xlp.info().await?;
            if info.code_id != liquidity_token_code_id {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                    contract_addr: xlp.get_address_string(),
                    new_code_id: liquidity_token_code_id,
                    msg: to_json_binary(
                        &perpswap::contracts::liquidity_token::entry::MigrateMsg {},
                    )?,
                }));
            }
        }

        let info = pos.info().await?;
        if let Some(position_token_code_id) = position_token_code_id {
            if info.code_id != position_token_code_id {
                msgs.push(CosmosMsg::Wasm(WasmMsg::Migrate {
                    contract_addr: pos.get_address_string(),
                    new_code_id: position_token_code_id,
                    msg: to_json_binary(
                        &perpswap::contracts::position_token::entry::MigrateMsg {},
                    )?,
                }));
            }
        }
    }

    if !msgs.is_empty() {
        tracing::info!("Migrate existing markets");
        tracing::info!("CW3 contract: {migration_admin}");
        signers.push(migration_admin);

        let chunks = msgs.chunks(30);
        let chunk_count = chunks.len();
        for (idx, msgs) in chunks.enumerate() {
            let idx = idx + 1;
            tracing::info!(
                "Message {idx}/{chunk_count}: {}",
                message_string(msgs, wrapped, &title, &desc)?
            );
            for msg in msgs {
                add_cosmos_msg(&mut builder, migration_admin, msg)?;
            }
        }
    }

    if signers.is_empty() {
        tracing::info!("No messages generated");
    } else {
        let res = builder
            .simulate(&app.cosmos, &signers)
            .await
            .context("Unable to simulate CW3 messages")?;
        tracing::info!("Successfully simulated messages, used {} gas", res.gas_used);
        tracing::debug!("Full simulate response: {res:?}");
    }

    Ok(())
}

fn message_string(
    msgs: &[CosmosMsg],
    wrapped: bool,
    title: &str,
    description: &str,
) -> Result<String> {
    Ok(if wrapped {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "snake_case")]
        enum Cw3Exec<'a> {
            Propose {
                title: String,
                description: String,
                msgs: &'a [CosmosMsg],
            },
        }
        serde_json::to_string(&Cw3Exec::Propose {
            title: title.to_owned(),
            description: description.to_owned(),
            msgs,
        })?
    } else {
        serde_json::to_string(msgs)?
    })
}
