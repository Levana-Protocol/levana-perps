use std::str::FromStr;

use anyhow::Result;
use cosmos::{Address, HasAddress};
use perps_exes::contracts::{Factory, MarketInfo};
use perpswap::contracts::{factory::entry::CodeIds, tracker::entry::ContractResp};

use crate::{
    cli::Opt,
    store_code::{COUNTER_TRADE, FACTORY, LIQUIDITY_TOKEN, MARKET, POSITION_TOKEN},
};

#[derive(clap::Parser)]
pub(crate) struct MigrateOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
    /// Update a specific sequence number? Refers to the sequence of the factory
    #[clap(long, env = "PERPS_SEQUENCE")]
    sequence: Option<u32>,
}

pub(crate) async fn go(opt: Opt, MigrateOpt { family, sequence }: MigrateOpt) -> Result<()> {
    let app = opt.load_app(&family).await?;
    let wallet = app.basic.get_wallet()?;

    let factory_code_id = app.tracker.require_code_by_type(&opt, FACTORY).await?;
    let position_token_code_id = app
        .tracker
        .require_code_by_type(&opt, POSITION_TOKEN)
        .await?;
    let liquidity_token_code_id = app
        .tracker
        .require_code_by_type(&opt, LIQUIDITY_TOKEN)
        .await?;
    let market_code_id = app.tracker.require_code_by_type(&opt, MARKET).await?;
    let countertrade_code_id = app
        .tracker
        .require_code_by_type(&opt, COUNTER_TRADE)
        .await?;

    let factory = match app
        .tracker
        .get_contract_by_family(FACTORY, &family, sequence)
        .await?
    {
        ContractResp::NotFound {} => anyhow::bail!("Factory contract not found"),
        ContractResp::Found { address, .. } => address.parse()?,
    };
    let factory = app.basic.cosmos.make_contract(factory);

    if app
        .basic
        .cosmos
        .make_contract(factory.get_address())
        .info()
        .await?
        .code_id
        == factory_code_id.get_code_id()
    {
        tracing::info!(
            "Factory's instantiated code ID is already {}, skipping",
            factory_code_id
        );
    } else {
        factory
            .migrate(
                wallet,
                factory_code_id.get_code_id(),
                perpswap::contracts::factory::entry::MigrateMsg {},
            )
            .await?;
        tracing::info!("Migrated the factory itself to {}", factory_code_id);
        let res = app
            .tracker
            .migrate(wallet, factory_code_id.get_code_id(), &factory)
            .await?;
        tracing::info!("Tracked factory migration in: {}", res.txhash);
    }

    let code_ids: CodeIds = factory
        .query(perpswap::contracts::factory::entry::QueryMsg::CodeIds {})
        .await?;

    if code_ids.liquidity_token.u64() == liquidity_token_code_id.get_code_id() {
        tracing::info!(
            "Liquidity token code ID in factory is already {}, skipping",
            liquidity_token_code_id
        );
    } else {
        let res = factory
            .execute(
                wallet,
                vec![],
                perpswap::contracts::factory::entry::ExecuteMsg::SetLiquidityTokenCodeId {
                    code_id: liquidity_token_code_id.get_code_id().to_string(),
                },
            )
            .await?;
        tracing::info!("Update liquidity token ID in factory: {}", res.txhash);
    }

    if code_ids.market.u64() == market_code_id.get_code_id() {
        tracing::info!(
            "Market code ID in factory is already {}, skipping",
            market_code_id
        );
    } else {
        let res = factory
            .execute(
                wallet,
                vec![],
                perpswap::contracts::factory::entry::ExecuteMsg::SetMarketCodeId {
                    code_id: market_code_id.get_code_id().to_string(),
                },
            )
            .await?;
        tracing::info!("Update market ID in factory: {}", res.txhash);
    }

    if code_ids.position_token.u64() == position_token_code_id.get_code_id() {
        tracing::info!(
            "Position token code ID in factory is already {}, skipping",
            position_token_code_id
        );
    } else {
        let res = factory
            .execute(
                wallet,
                vec![],
                perpswap::contracts::factory::entry::ExecuteMsg::SetPositionTokenCodeId {
                    code_id: position_token_code_id.get_code_id().to_string(),
                },
            )
            .await?;
        tracing::info!("Update position token ID in factory: {}", res.txhash);
    }

    if code_ids.counter_trade.map(|code_id| code_id.u64())
        == Some(countertrade_code_id.get_code_id())
    {
        tracing::info!(
            "Countertrade code ID in factory is already {countertrade_code_id}, skipping"
        )
    } else {
        let res = factory
            .execute(
                wallet,
                vec![],
                perpswap::contracts::factory::entry::ExecuteMsg::SetCounterTradeCodeId {
                    code_id: countertrade_code_id.get_code_id().to_string(),
                },
            )
            .await?;
        tracing::info!("Updated countertrade code ID in factory: {}", res.txhash);

        if code_ids.counter_trade.is_some() {
            // No need for migration if this was the first version
            let factory = Factory::from_contract(factory.clone());
            for (market_id, counter_trade) in factory.get_countertrade_address().await? {
                tracing::info!("Performing migration for countertrade ({market_id})");
                let counter_trade = Address::from_str(counter_trade.as_str())?;
                let contract = app.basic.cosmos.make_contract(counter_trade);
                contract
                    .migrate(
                        wallet,
                        countertrade_code_id.get_code_id(),
                        perpswap::contracts::countertrade::MigrateMsg {},
                    )
                    .await?;
                tracing::info!("Countertrade contract for {market_id} migrated");
            }
        }
    }

    let factory = Factory::from_contract(factory);

    for MarketInfo {
        market_id,
        market,
        position_token,
        liquidity_token_lp,
        liquidity_token_xlp,
    } in factory.get_markets().await?
    {
        tracing::info!("Performing migrations for market {market_id}");
        let current_market_code_id = market.info().await?.code_id;
        if current_market_code_id == market_code_id.get_code_id() {
            tracing::info!("Skipping market contract migration");
        } else {
            market
                .migrate(
                    wallet,
                    market_code_id.get_code_id(),
                    perpswap::contracts::market::entry::MigrateMsg {},
                )
                .await?;
            tracing::info!("Market contract for {market_id} migrated");
            match app
                .tracker
                .migrate(wallet, market_code_id.get_code_id(), market.get_address())
                .await
            {
                Err(e) => tracing::warn!(
                    "Unable to log tracker update for market contract {}: {e}",
                    market.get_address()
                ),
                Ok(res) => tracing::info!(
                    "Logged market {market_id} update in tracker at: {}",
                    res.txhash
                ),
            }
        }

        let current_position_code_id = position_token.info().await?.code_id;
        if current_position_code_id == position_token_code_id.get_code_id() {
            tracing::info!("Skipping migration of position token contract");
        } else {
            position_token
                .migrate(
                    wallet,
                    position_token_code_id.get_code_id(),
                    perpswap::contracts::position_token::entry::MigrateMsg {},
                )
                .await?;
            tracing::info!("Position token contract for {market_id} migrated");
            match app
                .tracker
                .migrate(
                    wallet,
                    position_token_code_id.get_code_id(),
                    position_token.get_address(),
                )
                .await
            {
                Err(e) => {
                    tracing::warn!(
                        "Unable to migrate position token contract {position_token}: {e}"
                    )
                }
                Ok(res) => tracing::info!(
                    "Logged position token {market_id} update in tracker at: {}",
                    res.txhash
                ),
            }
        }

        for (kind, lt) in [("LP", liquidity_token_lp), ("xLP", liquidity_token_xlp)] {
            if lt.info().await?.code_id == liquidity_token_code_id.get_code_id() {
                tracing::info!(
                    "Skipping {kind} liquidity token contract migration for {market_id}"
                );
            } else {
                lt.migrate(
                    wallet,
                    liquidity_token_code_id.get_code_id(),
                    perpswap::contracts::position_token::entry::MigrateMsg {},
                )
                .await?;
                tracing::info!("{kind} liquidity token contract for {market_id} migrated");
                match app
                    .tracker
                    .migrate(
                        wallet,
                        liquidity_token_code_id.get_code_id(),
                        lt.get_address(),
                    )
                    .await
                {
                    Err(e) => {
                        tracing::warn!(
                            "Unable to migrate {kind} liquidity token contract {lt}: {e}"
                        )
                    }
                    Ok(res) => tracing::info!(
                        "Logged {kind} liquidity token {market_id} update in tracker at: {}",
                        res.txhash
                    ),
                }
            }
        }

        // Check if the price admin is a pyth_bridge contract
        // let price_admin = app.basic.cosmos.make_contract(price_admin);
        // match price_admin.info().await {
        //     Err(e) => {
        //         tracing::info!("Received error when querying contract info for price admin {price_admin}: {e:?}");
        //         tracing::info!("Ignoring, assuming we're not using pyth_bridge");
        //     }
        //     Ok(info) => {
        //         // Check if this is a new Hermes Pyth bridge. If the config call
        //         // fails, we'll assume that it's an old bridge and we need to
        //         // insantiate a new one.
        //         let config_res: Result<perpswap::contracts::pyth_bridge::entry::Config> = price_admin
        //             .query(perpswap::contracts::pyth_bridge::entry::QueryMsg::Config {})
        //             .await;
        //         match config_res {
        //             Ok(_) => {
        //                 tracing::info!("Already on a Hermes Pyth bridge, will test migrating");
        //                 if info.code_id == pyth_bridge_code_id.get_code_id() {
        //                     tracing::info!(
        //                 "Price admin {price_admin} is already using current Pyth bridge code ID {}",
        //                 info.code_id
        //             );
        //                 } else {
        //                     price_admin
        //                         .migrate(
        //                             &app.basic.wallet,
        //                             pyth_bridge_code_id.get_code_id(),
        //                             perpswap::contracts::pyth_bridge::entry::MigrateMsg {},
        //                         )
        //                         .await?;
        //                     tracing::info!("pyth_bridge price admin contract for {market_id} migrated");
        //                     match app
        //                         .tracker
        //                         .migrate(
        //                             &app.basic.wallet,
        //                             pyth_bridge_code_id.get_code_id(),
        //                             price_admin.get_address(),
        //                         )
        //                         .await
        //                         {
        //                             Err(e) => {
        //                                 tracing::warn!("Unable to migrate pyth_bridge price admin contract: {e:?}")
        //                             }
        //                             Ok(res) => tracing::info!(
        //                                 "Logged pyth_bridge price admin contract for {market_id}, update in tracker at: {}",
        //                                 res.txhash
        //                             ),
        //                         }
        //                 }
        //             }
        //             Err(_) => {
        //                 tracing::info!("Not on a Hermes Pyth bridge yet, need to instantiate a new bridge and set the owner.");
        //                 let pyth_info = match &app.price_source {
        //                     crate::app::PriceSourceConfig::Pyth(pyth_info) => pyth_info,
        //                     crate::app::PriceSourceConfig::Wallet(_) => anyhow::bail!("Cannot instantiate new Hermes Pyth bridge, PriceSourceConfig is Wallet"),
        //                 };
        //                 let pyth_bridge = pyth_info
        //                     .make_pyth_bridge(
        //                         pyth_bridge_code_id.clone(),
        //                         &app.basic.wallet,
        //                         &factory,
        //                         market_id,
        //                     )
        //                     .await?;
        //                 tracing::info!("Deployed fresh Pyth bridge contract: {pyth_bridge}");
        //                 let tx = factory
        //                     .set_price_admin(&app.basic.wallet, &market, &pyth_bridge)
        //                     .await?;
        //                 tracing::info!("Updated price admin in transaction {}", tx.txhash);
        //             }
        //         }
        //     }
        // }
    }

    Ok(())
}
