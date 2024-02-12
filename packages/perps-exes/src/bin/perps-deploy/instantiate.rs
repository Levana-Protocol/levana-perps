use std::collections::HashSet;

use anyhow::{Context, Result};
use cosmos::{Address, CodeId, ContractAdmin, Cosmos, HasAddress, Wallet};
use msg::contracts::market::spot_price::{PythConfigInit, SpotPriceConfigInit, StrideConfigInit};
use msg::prelude::*;
use msg::{
    contracts::{
        cw20::Cw20Coin,
        market::{config::ConfigUpdate, entry::NewMarketParams},
    },
    token::TokenInit,
};
use perps_exes::config::{ConfigTestnet, ConfigUpdateAndBorrowFee, MarketConfigUpdates};
use perps_exes::contracts::{Factory, MarketInfo};
use perps_exes::prelude::MarketContract;

use crate::app::{App, PriceSourceConfig};
use crate::{
    app::BasicApp,
    cli::Opt,
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
    #[clap(long, env = "PERPS_MARKET_ID")]
    pub(crate) market_id: Option<Vec<MarketId>>,
}

impl App {
    pub(crate) fn make_instantiate_market(&self, market_id: MarketId) -> Result<InstantiateMarket> {
        let ConfigUpdateAndBorrowFee {
            mut config,
            initial_borrow_fee_rate,
        } = MarketConfigUpdates::load(&self.market_config)?
            .markets
            .get(&market_id)
            .with_context(|| format!("No MarketConfigUpdate found for {market_id}"))?
            .clone();

        if self.dev_settings {
            config.unstake_period_seconds = Some(60 * 60);
        }
        Ok(InstantiateMarket {
            // TODO - maybe make this configurable via yaml files
            collateral: match market_id.as_str() {
                "nBTC_USD" => CollateralSource::Native {
                    denom: "ibc/5946AD5E947FF47B521103044C74B6FC3DD242227433EE9278F2B044B2AA2DF0"
                        .to_string(),
                    decimal_places: 14,
                },
                _ => CollateralSource::Cw20(Cw20Source::Faucet(self.faucet.clone())),
            },
            config,
            initial_borrow_fee_rate,
            spot_price: match &self.price_source {
                PriceSourceConfig::Wallet(admin) => SpotPriceConfigInit::Manual {
                    admin: admin.get_address_string().into(),
                },
                PriceSourceConfig::Oracle(oracle) => {
                    let market = oracle
                        .markets
                        .get(&market_id)
                        .with_context(|| format!("No oracle market found for {market_id}"))?;
                    let stride = match market.stride_contract_override {
                        Some(stride) => Some(stride),
                        None => oracle.stride_fallback.clone().map(|x| x.contract),
                    };

                    SpotPriceConfigInit::Oracle {
                        pyth: oracle.pyth.as_ref().map(|pyth| PythConfigInit {
                            contract_address: pyth.contract.get_address_string().into(),
                            network: pyth.r#type,
                        }),
                        stride: stride.map(|addr| StrideConfigInit {
                            contract_address: addr.get_address_string().into(),
                        }),
                        feeds: market
                            .feeds
                            .iter()
                            .map(|feed| feed.clone().into())
                            .collect(),
                        feeds_usd: market
                            .feeds_usd
                            .iter()
                            .map(|feed| feed.clone().into())
                            .collect(),
                        volatile_diff_seconds: None,
                    }
                }
            },
            market_id,
        })
    }
}

pub(crate) async fn go(opt: Opt, inst_opt: InstantiateOpt) -> Result<()> {
    let app = opt.load_app(&inst_opt.family).await?;

    instantiate(InstantiateParams {
        opt: &opt,
        basic: &app.basic,
        config_testnet: &app.config_testnet,
        family: inst_opt.family,
        markets: inst_opt
            .market_id
            .unwrap_or_else(|| app.default_market_ids.clone())
            .into_iter()
            .map(|market_id| app.make_instantiate_market(market_id))
            .collect::<Result<_>>()?,
        code_id_source: CodeIdSource::Tracker(app.tracker),
        trading_competition: app.trading_competition,
        faucet_admin: Some(app.wallet_manager),
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
    pub(crate) collateral: CollateralResponse,
}

#[derive(serde::Serialize)]
pub(crate) enum CollateralResponse {
    Cw20(Address),
    Native { denom: String, decimal_places: u8 },
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
    pub(crate) config_testnet: &'a ConfigTestnet,
    pub(crate) code_id_source: CodeIdSource,
    pub(crate) family: String,
    pub(crate) markets: Vec<InstantiateMarket>,
    pub(crate) trading_competition: bool,
    /// Address that should be set as a faucet admin
    pub(crate) faucet_admin: Option<Address>,
}

pub(crate) struct InstantiateMarket {
    pub(crate) market_id: MarketId,
    pub(crate) collateral: CollateralSource,
    pub(crate) config: ConfigUpdate,
    pub(crate) initial_borrow_fee_rate: Decimal256,
    pub(crate) spot_price: SpotPriceConfigInit,
}

pub(crate) enum CollateralSource {
    Cw20(Cw20Source),
    Native { denom: String, decimal_places: u8 },
}

pub(crate) async fn instantiate(
    InstantiateParams {
        opt,
        basic,
        config_testnet,
        code_id_source,
        trading_competition,
        faucet_admin,
        markets,
        family,
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
    let label_suffix = format!(" - {family}");

    let wallet = basic.get_wallet()?;
    let cosmos = &basic.cosmos;

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
            ContractAdmin::Sender,
        )
        .await?;
    let factory = Factory::from_contract(factory);
    log::info!("New factory deployed at {factory}");
    to_log.push((factory_code_id.get_code_id(), factory.get_address()));

    let mut market_res = Vec::<(MarketResponse, SpotPriceConfigInit)>::new();

    for market in markets {
        let spot_price = market.spot_price.clone();
        let res = market
            .add(
                wallet,
                cosmos,
                config_testnet,
                AddMarketParams {
                    trading_competition,
                    faucet_admin,
                    factory: factory.clone(),
                },
            )
            .await?;
        market_res.push((res, spot_price));
    }

    let factory_addr = factory.get_address();
    for MarketInfo {
        market_id: _,
        market,
        position_token,
        liquidity_token_lp,
        liquidity_token_xlp,
    } in factory.get_markets().await?
    {
        to_log.push((market_code_id.get_code_id(), market.get_address()));
        to_log.push((
            position_token_code_id.get_code_id(),
            position_token.get_address(),
        ));
        to_log.push((
            liquidity_token_code_id.get_code_id(),
            liquidity_token_lp.get_address(),
        ));
        to_log.push((
            liquidity_token_code_id.get_code_id(),
            liquidity_token_xlp.get_address(),
        ));
    }

    if let Some(tracker) = tracker {
        let res = tracker.instantiate(wallet, &to_log, family).await?;
        log::info!("Logged new contracts in tracker at {}", res.txhash);
    }

    // Sanity check the markets. Might be removed one day, but for now it's helpful to debug.
    for (market, spot_price) in market_res.iter() {
        match spot_price {
            SpotPriceConfigInit::Manual { .. } => {
                log::info!("not doing initial crank for {} because it's a manual market so an initial price must be added first", market.market_id);
            }
            SpotPriceConfigInit::Oracle {
                feeds, feeds_usd, ..
            } => {
                if feeds.iter().chain(feeds_usd.iter()).any(|f| {
                    matches!(
                        f.data,
                        msg::contracts::market::spot_price::SpotPriceFeedDataInit::Pyth { .. }
                    )
                }) {
                    log::info!("not doing initial crank for {} because it contains pyth feeds which may need a publish first", market.market_id);
                } else {
                    log::info!("doing initial crank to sanity check that spot price oracle is working for {}", market.market_id);
                    let contract = cosmos.make_contract(market.market_addr);
                    contract
                        .execute(
                            wallet,
                            vec![],
                            msg::contracts::market::entry::ExecuteMsg::Crank {
                                execs: None,
                                rewards: None,
                            },
                        )
                        .await?;
                }
            }
        }
    }

    log::info!("Done!");

    Ok(InstantiateResponse {
        factory: factory_addr,
        markets: market_res.into_iter().map(|(market, _)| market).collect(),
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

pub(crate) struct AddMarketParams {
    pub(crate) trading_competition: bool,
    pub(crate) faucet_admin: Option<Address>,
    pub(crate) factory: Factory,
}

impl InstantiateMarket {
    pub(crate) async fn add(
        self,
        wallet: &Wallet,
        cosmos: &Cosmos,
        config_testnet: &ConfigTestnet,
        AddMarketParams {
            trading_competition,
            faucet_admin,
            factory,
        }: AddMarketParams,
    ) -> Result<MarketResponse> {
        let InstantiateMarket {
            market_id,
            collateral,
            config,
            initial_borrow_fee_rate,
            spot_price,
        } = self;

        let (collateral, trading_competition) = match collateral {
            CollateralSource::Cw20(cw20_source) => {
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
                            let address = match faucet
                                .get_cw20(market_id.get_collateral(), None)
                                .await?
                            {
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

                        if let Some(new_admin) = faucet_admin {
                            // Try adding the wallet manager address as a faucet admin. Ignore errors, it
                            // (probably) just means we've already added that address.
                            if faucet.is_admin(new_admin).await? {
                                log::info!(
                                    "{new_admin} is already a faucet admin for {}",
                                    faucet.get_address()
                                );
                            } else {
                                log::info!(
                                    "Trying to set {new_admin} as a faucet admin on {}",
                                    faucet.get_address()
                                );
                                let res = faucet.add_admin(wallet, new_admin).await?;
                                log::info!("Admin set in {}", res.txhash);
                            }
                        }

                        let res = faucet
                            .mint(wallet, cw20, make_initial_balances(&[wallet.get_address()]))
                            .await?;
                        log::info!("Minted in {}", res.txhash);
                        (cw20, trading_competition.map(|index| (index, faucet)))
                    }
                };
                log::info!("Using CW20 {cw20}");

                let cw20 = cosmos.make_contract(cw20);

                (
                    CollateralResponse::Cw20(cw20.get_address()),
                    trading_competition,
                )
            }
            CollateralSource::Native {
                denom,
                decimal_places,
            } => {
                log::info!("Using native denom {denom} for market {market_id}",);
                anyhow::ensure!(
                    !trading_competition,
                    "Cannot use native denom with trading competition"
                );
                (
                    CollateralResponse::Native {
                        denom,
                        decimal_places,
                    },
                    None,
                )
            }
        };
        let initial_price = match &spot_price {
            SpotPriceConfigInit::Manual { .. } => Some(
                *config_testnet
                    .initial_prices
                    .get(&market_id)
                    .with_context(|| format!("No initial prices found for {market_id}"))?,
            ),
            SpotPriceConfigInit::Oracle { .. } => None,
        };

        let res = factory
            .add_market(
                wallet,
                NewMarketParams {
                    market_id: market_id.clone(),
                    token: match &collateral {
                        CollateralResponse::Cw20(addr) => TokenInit::Cw20 {
                            addr: addr.to_string().into(),
                        },
                        CollateralResponse::Native {
                            denom,
                            decimal_places,
                        } => TokenInit::Native {
                            denom: denom.clone(),
                            decimal_places: *decimal_places,
                        },
                    },
                    config: Some(config),
                    initial_borrow_fee_rate,
                    spot_price,
                    initial_price,
                },
            )
            .await
            .with_context(|| format!("Adding new market {market_id}"))?;
        log::info!("Market {market_id} added at {}", res.txhash);

        let MarketInfo { market, .. } = factory.get_market(market_id.clone()).await?;
        let market_addr = market.get_address();
        log::info!("New market address for {market_id}: {market_addr}");

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

            let market = MarketContract::new(cosmos.make_contract(market_addr));

            let res = market
                .config_update(
                    wallet,
                    ConfigUpdate {
                        disable_position_nft_exec: Some(true),
                        ..Default::default()
                    },
                )
                .await?;

            log::info!(
                "Disabled NFT executions for new trading competition at {}",
                res.txhash
            );

            let res = factory.disable_trades(wallet, market_id.clone()).await?;
            log::info!("Market shut down in {}", res.txhash);
        }

        Ok(MarketResponse {
            market_id,
            market_addr,
            collateral,
        })
    }
}
