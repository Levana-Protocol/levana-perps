use anyhow::Result;
use cosmos::HasAddress;
use msg::contracts::{factory::entry::CodeIds, tracker::entry::ContractResp};

use crate::{
    cli::Opt,
    factory::{Factory, MarketInfo},
    store_code::{FACTORY, LIQUIDITY_TOKEN, MARKET, POSITION_TOKEN},
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
        .contract_info(factory.get_address_string())
        .await?
        .code_id
        == factory_code_id.get_code_id()
    {
        log::info!(
            "Factory's instantiated code ID is already {}, skipping",
            factory_code_id
        );
    } else {
        factory
            .migrate(
                &app.basic.wallet,
                factory_code_id.get_code_id(),
                msg::contracts::factory::entry::MigrateMsg {},
            )
            .await?;
        log::info!("Migrated the factory itself to {}", factory_code_id);
        let res = app
            .tracker
            .migrate(&app.basic.wallet, factory_code_id.get_code_id(), &factory)
            .await?;
        log::info!("Tracked factory migration in: {}", res.txhash);
    }

    let code_ids: CodeIds = factory
        .query(msg::contracts::factory::entry::QueryMsg::CodeIds {})
        .await?;

    if code_ids.liquidity_token.u64() == liquidity_token_code_id.get_code_id() {
        log::info!(
            "Liquidity token code ID in factory is already {}, skipping",
            liquidity_token_code_id
        );
    } else {
        let res = factory
            .execute(
                &app.basic.wallet,
                vec![],
                msg::contracts::factory::entry::ExecuteMsg::SetLiquidityTokenCodeId {
                    code_id: liquidity_token_code_id.get_code_id().to_string(),
                },
            )
            .await?;
        log::info!("Update liquidity token ID in factory: {}", res.txhash);
    }

    if code_ids.market.u64() == market_code_id.get_code_id() {
        log::info!(
            "Market code ID in factory is already {}, skipping",
            market_code_id
        );
    } else {
        let res = factory
            .execute(
                &app.basic.wallet,
                vec![],
                msg::contracts::factory::entry::ExecuteMsg::SetMarketCodeId {
                    code_id: market_code_id.get_code_id().to_string(),
                },
            )
            .await?;
        log::info!("Update market ID in factory: {}", res.txhash);
    }

    if code_ids.position_token.u64() == position_token_code_id.get_code_id() {
        log::info!(
            "Position token code ID in factory is already {}, skipping",
            position_token_code_id
        );
    } else {
        let res = factory
            .execute(
                &app.basic.wallet,
                vec![],
                msg::contracts::factory::entry::ExecuteMsg::SetPositionTokenCodeId {
                    code_id: position_token_code_id.get_code_id().to_string(),
                },
            )
            .await?;
        log::info!("Update position token ID in factory: {}", res.txhash);
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
        log::info!("Performing migrations for market {market_id}");
        let current_market_code_id = market.info().await?.code_id;
        if current_market_code_id == market_code_id.get_code_id() {
            log::info!("Skipping market contract migration");
        } else {
            market
                .migrate(
                    &app.basic.wallet,
                    market_code_id.get_code_id(),
                    msg::contracts::market::entry::MigrateMsg {},
                )
                .await?;
            log::info!("Market contract for {market_id} migrated");
            match app
                .tracker
                .migrate(
                    &app.basic.wallet,
                    market_code_id.get_code_id(),
                    market.get_address(),
                )
                .await
            {
                Err(e) => log::warn!(
                    "Unable to log tracker update for market contract {}: {e:?}",
                    market.get_address()
                ),
                Ok(res) => log::info!(
                    "Logged market {market_id} update in tracker at: {}",
                    res.txhash
                ),
            }
        }

        let current_position_code_id = position_token.info().await?.code_id;
        if current_position_code_id == position_token_code_id.get_code_id() {
            log::info!("Skipping migration of position token contract");
        } else {
            position_token
                .migrate(
                    &app.basic.wallet,
                    position_token_code_id.get_code_id(),
                    msg::contracts::position_token::entry::MigrateMsg {},
                )
                .await?;
            log::info!("Position token contract for {market_id} migrated");
            match app
                .tracker
                .migrate(
                    &app.basic.wallet,
                    position_token_code_id.get_code_id(),
                    position_token.get_address(),
                )
                .await
            {
                Err(e) => {
                    log::warn!("Unable to migrate position token contract {position_token}: {e:?}")
                }
                Ok(res) => log::info!(
                    "Logged position token {market_id} update in tracker at: {}",
                    res.txhash
                ),
            }
        }

        for (kind, lt) in [("LP", liquidity_token_lp), ("xLP", liquidity_token_xlp)] {
            if lt.info().await?.code_id == liquidity_token_code_id.get_code_id() {
                log::info!("Skipping {kind} liquidity token contract migration for {market_id}");
            } else {
                lt.migrate(
                    &app.basic.wallet,
                    liquidity_token_code_id.get_code_id(),
                    msg::contracts::position_token::entry::MigrateMsg {},
                )
                .await?;
                log::info!("{kind} liquidity token contract for {market_id} migrated");
                match app
                    .tracker
                    .migrate(
                        &app.basic.wallet,
                        liquidity_token_code_id.get_code_id(),
                        lt.get_address(),
                    )
                    .await
                {
                    Err(e) => {
                        log::warn!("Unable to migrate {kind} liquidity token contract {lt}: {e:?}")
                    }
                    Ok(res) => log::info!(
                        "Logged {kind} liquidity token {market_id} update in tracker at: {}",
                        res.txhash
                    ),
                }
            }
        }
    }

    Ok(())
}
