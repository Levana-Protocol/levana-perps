use std::io::Write;

use anyhow::{Context, Result};
use cosmos::{ContractAdmin, CosmosNetwork, HasAddress};
use msg::contracts::{
    cw20::{entry::InstantiateMinter, Cw20Coin},
    market::{config::ConfigUpdate, spot_price::SpotPriceConfigInit},
};
use msg::prelude::*;
use perps_exes::{config::ConfigTestnet, PerpsNetwork};

use crate::{
    cli::Opt,
    instantiate::{
        InstantiateMarket, InstantiateParams, InstantiateResponse, MarketResponse, ProtocolCodeIds,
        INITIAL_BALANCE_AMOUNT,
    },
    store_code::{CW20, FACTORY, LIQUIDITY_TOKEN, MARKET, POSITION_TOKEN},
};

#[derive(clap::Parser)]
pub(crate) struct LocalDeployOpt {
    /// Network to use. Either this or family must be provided.
    #[clap(long, env = "COSMOS_NETWORK")]
    pub(crate) network: PerpsNetwork,
    /// Initial price to set the market to.
    ///
    /// Provided as a convenience to make local testing easier.
    #[clap(long, env = "PERPS_INITIAL_PRICE", default_value = "9.5")]
    pub(crate) initial_price: PriceBaseInQuote,
    /// Initial collateral price for markets without USD as notional.
    #[clap(long, env = "PERPS_COLLATERAL_PRICE", default_value = "20")]
    pub(crate) collateral_price: PriceCollateralInUsd,
}

pub(crate) async fn go(
    opt: Opt,
    LocalDeployOpt {
        network,
        initial_price,
        collateral_price,
    }: LocalDeployOpt,
) -> Result<InstantiateResponse> {
    let basic = opt.load_basic_app(network).await?;
    let wallet = basic.get_wallet()?;
    let config_testnet = ConfigTestnet::load(opt.config_testnet.as_ref())?;

    match network {
        PerpsNetwork::Regular(
            CosmosNetwork::JunoLocal | CosmosNetwork::OsmosisLocal | CosmosNetwork::WasmdLocal,
        ) => (),
        _ => anyhow::bail!("Please only use local deploy for a local --network"),
    }

    let market_id: Vec<MarketId> = vec![
        "ATOM_USD".parse()?,
        "OSMO_USDC".parse()?,
        "ETH_BTC".parse()?,
    ];

    // Deploy a fresh tracker to local
    let cw20_code_id = basic
        .cosmos
        .store_code_path(wallet, opt.get_contract_path(CW20))
        .await?;
    let mut markets = Vec::<InstantiateMarket>::new();
    for market_id in market_id {
        let cw20 = cw20_code_id
            .instantiate(
                wallet,
                "CW20",
                vec![],
                msg::contracts::cw20::entry::InstantiateMsg {
                    name: market_id.get_collateral().to_owned(),
                    symbol: market_id.get_collateral().to_owned(),
                    decimals: 6,
                    initial_balances: vec![Cw20Coin {
                        address: wallet.get_address_string(),
                        amount: INITIAL_BALANCE_AMOUNT.into(),
                    }],
                    minter: InstantiateMinter {
                        minter: wallet.get_address_string().into(),
                        cap: None,
                    },
                    marketing: None,
                },
                ContractAdmin::Sender,
            )
            .await?;

        log::info!(
            "New CW20 address for {} is {cw20} with code ID {cw20_code_id}",
            market_id.get_collateral()
        );

        markets.push(InstantiateMarket {
            market_id,
            collateral: crate::instantiate::CollateralSource::Cw20(
                crate::instantiate::Cw20Source::Existing(cw20.get_address()),
            ),
            config: ConfigUpdate::default(),
            initial_borrow_fee_rate: "0.01".parse().unwrap(),
            spot_price: SpotPriceConfigInit::Manual {
                admin: wallet.get_address_string().into(),
            },
        });
    }

    let ids = ProtocolCodeIds {
        factory_code_id: basic
            .cosmos
            .store_code_path(wallet, opt.get_contract_path(FACTORY))
            .await?,
        position_token_code_id: basic
            .cosmos
            .store_code_path(wallet, opt.get_contract_path(POSITION_TOKEN))
            .await?,
        liquidity_token_code_id: basic
            .cosmos
            .store_code_path(wallet, opt.get_contract_path(LIQUIDITY_TOKEN))
            .await?,
        market_code_id: basic
            .cosmos
            .store_code_path(wallet, opt.get_contract_path(MARKET))
            .await?,
    };

    // And now instantiate the contracts
    log::info!("Instantiating contracts");
    let res = crate::instantiate::instantiate(InstantiateParams {
        opt: &opt,
        basic: &basic,
        config_testnet: &config_testnet,
        code_id_source: crate::instantiate::CodeIdSource::Existing(ids),
        family: "localperps".to_owned(),
        markets,
        trading_competition: false,
        faucet_admin: None,
    })
    .await?;

    // Set the price for the markets.
    for MarketResponse {
        market_id,
        market_addr,
        collateral: _,
    } in &res.markets
    {
        let set_price = basic
            .cosmos
            .make_contract(*market_addr)
            .execute(
                wallet,
                vec![],
                msg::contracts::market::entry::ExecuteMsg::SetManualPrice {
                    price: initial_price,
                    price_usd: initial_price
                        .try_into_usd(market_id)
                        .unwrap_or(collateral_price),
                },
            )
            .await
            .context("Unable to set price")?;
        log::info!(
            "Set initial price in market {} to {initial_price} in {}",
            market_id,
            set_price.txhash
        );

        // Wait until the new price is in the system
        for _ in 0..100 {
            let spot_price: Result<PricePoint, _> = basic
                .cosmos
                .make_contract(*market_addr)
                .query(msg::contracts::market::entry::QueryMsg::SpotPrice { timestamp: None })
                .await;
            match spot_price {
                Ok(spot_price) => {
                    log::info!("New spot price {spot_price:?} active in contract");
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, &res)?;
    stdout.flush()?;

    Ok(res)
}
