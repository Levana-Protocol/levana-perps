use std::{collections::HashSet, str::FromStr};

use anyhow::{Context, Result};
use cosmos::{Address, CodeId, HasAddress};
use msg::prelude::*;
use msg::{
    contracts::{
        cw20::Cw20Coin,
        factory::entry::MarketInfoResponse,
        market::{config::ConfigUpdate, entry::NewMarketParams},
    },
    token::TokenInit,
};

use crate::{
    app::BasicApp,
    cli::Opt,
    factory::{Factory, MarketInfo},
    faucet::Faucet,
    store_code::{FACTORY, LIQUIDITY_TOKEN, MARKET, POSITION_TOKEN},
    tracker::Tracker,
};

#[derive(clap::Parser)]
pub(crate) struct InstantiateOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    pub(crate) family: String,
    /// Markets to instantiate
    #[clap(long, env = "PERPS_MARKET_ID", default_value = "ATOM_USD")]
    pub(crate) market_id: Vec<MarketId>,
    /// Is this a production deployment? Impacts labels used
    #[clap(long)]
    pub(crate) prod: bool,
    /// Initial borrow fee rate
    #[clap(long, default_value = "0.2")]
    pub(crate) initial_borrow_fee_rate: Decimal256,
}

pub(crate) async fn go(opt: Opt, inst_opt: InstantiateOpt) -> Result<()> {
    let app = opt.load_app(&inst_opt.family).await?;
    let is_prod = inst_opt.prod;

    instantiate(InstantiateParams {
        opt: &opt,
        basic: &app.basic,
        code_id_source: CodeIdSource::Tracker(app.tracker),
        family: inst_opt.family,
        markets: inst_opt
            .market_id
            .into_iter()
            .map(|market_id| InstantiateMarket {
                market_id,
                cw20_source: Cw20Source::Faucet(app.faucet.clone()),
            })
            .collect(),
        market_config_update: if app.dev_settings {
            Some(ConfigUpdate {
                unstake_period_seconds: Some(60 * 60),
                ..Default::default()
            })
        } else {
            None
        },
        trading_competition: app.trading_competition,
        nibb: app.nibb,
        price: app.price,
        is_prod,
        initial_borrow_fee_rate: inst_opt.initial_borrow_fee_rate,
    })
    .await?;

    Ok(())
}

#[derive(serde::Serialize)]
pub(crate) struct InstantiateResponse {
    pub(crate) factory: Address,
    pub(crate) markets: Vec<MarketResponse>,
}

#[derive(serde::Serialize)]
pub(crate) struct MarketResponse {
    pub(crate) market_id: MarketId,
    pub(crate) market_addr: Address,
    pub(crate) cw20: Address,
}

pub(crate) enum Cw20Source {
    Existing(Address),
    Faucet(Faucet),
}

pub(crate) enum CodeIdSource {
    Tracker(Tracker),
    Existing(ProtocolCodeIds),
}

pub(crate) struct ProtocolCodeIds {
    pub(crate) factory_code_id: CodeId,
    pub(crate) position_token_code_id: CodeId,
    pub(crate) liquidity_token_code_id: CodeId,
    pub(crate) market_code_id: CodeId,
}

/// Parameters to instantiate, used to avoid too many function parameters.
pub(crate) struct InstantiateParams<'a> {
    pub(crate) opt: &'a Opt,
    pub(crate) basic: &'a BasicApp,
    pub(crate) code_id_source: CodeIdSource,
    pub(crate) family: String,
    pub(crate) markets: Vec<InstantiateMarket>,
    pub(crate) market_config_update: Option<ConfigUpdate>,
    pub(crate) trading_competition: bool,
    pub(crate) nibb: Address,
    pub(crate) price: Address,
    /// Is this a production contract? If so, does not include the family in the label
    pub(crate) is_prod: bool,
    /// Initial borrow fee rate
    pub(crate) initial_borrow_fee_rate: Decimal256,
}

pub(crate) struct InstantiateMarket {
    pub(crate) market_id: MarketId,
    pub(crate) cw20_source: Cw20Source,
}

pub(crate) async fn instantiate(
    InstantiateParams {
        opt,
        basic:
            BasicApp {
                cosmos,
                wallet,
                network: _,
                chain_config: _,
            },
        code_id_source,
        market_config_update,
        trading_competition,
        nibb,
        price,
        is_prod,
        markets,
        family,
        initial_borrow_fee_rate,
    }: InstantiateParams<'_>,
) -> Result<InstantiateResponse> {
    let (
        tracker,
        ProtocolCodeIds {
            factory_code_id,
            position_token_code_id,
            liquidity_token_code_id,
            market_code_id,
        },
    ) = match code_id_source {
        CodeIdSource::Tracker(tracker) => {
            let ids = ProtocolCodeIds {
                factory_code_id: tracker.require_code_by_type(opt, FACTORY).await?,
                position_token_code_id: tracker.require_code_by_type(opt, POSITION_TOKEN).await?,
                liquidity_token_code_id: tracker.require_code_by_type(opt, LIQUIDITY_TOKEN).await?,
                market_code_id: tracker.require_code_by_type(opt, MARKET).await?,
            };
            (Some(tracker), ids)
        }
        CodeIdSource::Existing(ids) => (None, ids),
    };

    let mut to_log: Vec<(u64, Address)> = vec![];
    let label_suffix = if is_prod {
        "".to_owned()
    } else {
        format!(" - {family}")
    };

    let factory = factory_code_id
        .instantiate(
            wallet,
            format!("Levana Perps Factory{label_suffix}"),
            vec![],
            msg::contracts::factory::entry::InstantiateMsg {
                market_code_id: market_code_id.get_code_id().to_string(),
                position_token_code_id: position_token_code_id.get_code_id().to_string(),
                liquidity_token_code_id: liquidity_token_code_id.get_code_id().to_string(),
                migration_admin: wallet.get_address_string().into(),
                owner: wallet.get_address_string().into(),
                dao: wallet.get_address_string().into(),
                kill_switch: wallet.get_address_string().into(),
                wind_down: wallet.get_address_string().into(),
                label_suffix: Some(label_suffix),
            },
        )
        .await?;
    log::info!("New factory deployed at {factory}");
    to_log.push((factory_code_id.get_code_id(), *factory.get_address()));

    let mut market_res = Vec::<MarketResponse>::new();

    for InstantiateMarket {
        market_id,
        cw20_source,
    } in markets
    {
        log::info!(
            "Finding CW20 for collateral asset {} for market {market_id}",
            market_id.get_collateral()
        );
        let (cw20, trading_competition) = match cw20_source {
            Cw20Source::Existing(address) => {
                anyhow::ensure!(
                    !trading_competition,
                    "Cannot use existing CW20 with trading competition"
                );
                (address, None)
            }
            Cw20Source::Faucet(faucet) => {
                let (cw20, trading_competition) = if trading_competition {
                    log::info!("Trading competition, creating a fresh CW20");
                    let index = faucet
                        .next_trading_index(market_id.get_collateral())
                        .await?;
                    faucet
                        .deploy_token(wallet, market_id.get_collateral(), Some(index))
                        .await?;
                    let address = faucet
                        .get_cw20(market_id.get_collateral(), Some(index))
                        .await?
                        .context(
                            "CW20 for trading competition still not available after deploying it",
                        )?;
                    (address, Some(index))
                } else {
                    let address = match faucet.get_cw20(market_id.get_collateral(), None).await? {
                        Some(addr) => {
                            log::info!("Using existing CW20");
                            addr
                        }
                        None => {
                            log::info!("Deploying fresh CW20");
                            faucet
                                .deploy_token(wallet, market_id.get_collateral(), None)
                                .await?;
                            faucet
                                .get_cw20(market_id.get_collateral(), None)
                                .await?
                                .context("CW20 still not available after deploying it")?
                        }
                    };
                    (address, None)
                };

                let res = faucet
                    .mint(
                        wallet,
                        cw20,
                        make_initial_balances(&[*nibb.get_address(), *wallet.get_address()]),
                    )
                    .await?;
                log::info!("Minted in {}", res.txhash);
                (cw20, trading_competition.map(|index| (index, faucet)))
            }
        };
        log::info!("Using CW20 {cw20}");

        let cw20 = cosmos.make_contract(cw20);

        let res = factory
            .execute(
                wallet,
                vec![],
                msg::contracts::factory::entry::ExecuteMsg::AddMarket {
                    new_market: NewMarketParams {
                        market_id: market_id.clone(),
                        token: TokenInit::Cw20 {
                            addr: cw20.get_address_string().into(),
                        },
                        config: market_config_update.clone(),
                        price_admin: price.to_string().into(),
                        initial_borrow_fee_rate,
                    },
                },
            )
            .await?;
        log::info!("Market added at {}", res.txhash);

        let MarketInfoResponse { market_addr, .. } = factory
            .query(msg::contracts::factory::entry::QueryMsg::MarketInfo {
                market_id: market_id.clone(),
            })
            .await?;
        log::info!("New market address: {market_addr}");
        let market_addr = Address::from_str(market_addr.as_str())?;

        if let Some((trading_competition_index, faucet)) = trading_competition {
            let res = faucet
                .set_market_address(
                    wallet,
                    market_id.get_collateral(),
                    trading_competition_index,
                    market_addr,
                )
                .await?;
            log::info!(
                "Set market on the new trading competition CW20 at {}",
                res.txhash
            );
        }

        market_res.push(MarketResponse {
            market_id,
            market_addr,
            cw20: *cw20.get_address(),
        })
    }

    let factory_addr = *factory.get_address();
    for MarketInfo {
        market_id: _,
        market,
        position_token,
        liquidity_token_lp,
        liquidity_token_xlp,
    } in Factory::from_contract(factory).get_markets().await?
    {
        to_log.push((market_code_id.get_code_id(), *market.get_address()));
        to_log.push((
            position_token_code_id.get_code_id(),
            *position_token.get_address(),
        ));
        to_log.push((
            liquidity_token_code_id.get_code_id(),
            *liquidity_token_lp.get_address(),
        ));
        to_log.push((
            liquidity_token_code_id.get_code_id(),
            *liquidity_token_xlp.get_address(),
        ));
    }

    if let Some(tracker) = tracker {
        let res = tracker.instantiate(wallet, &to_log, family).await?;
        log::info!("Logged new contracts in tracker at {}", res.txhash);
    }

    Ok(InstantiateResponse {
        factory: factory_addr,
        markets: market_res,
    })
}

/// Handles de-duping of addresses
fn make_initial_balances(addrs: &[Address]) -> Vec<Cw20Coin> {
    addrs
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|address| Cw20Coin {
            address: address.get_address_string(),
            amount: INITIAL_BALANCE_AMOUNT.into(),
        })
        .collect()
}

pub(crate) const INITIAL_BALANCE_AMOUNT: u128 = 1_000_000_000_000_000_000u128;
