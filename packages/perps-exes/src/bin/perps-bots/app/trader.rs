use std::sync::Arc;

use anyhow::Result;
use cosmos::{Address, Cosmos, HasAddress, Wallet};
use msg::prelude::*;
use parking_lot::RwLock;
use perps_exes::config::DeploymentConfig;
use rand::Rng;
use tokio::sync::Mutex;

use crate::market_contract::MarketContract;

use super::{factory::FactoryInfo, status_collector::StatusCollector};

pub(super) struct Trader {
    pub(super) cosmos: Cosmos,
    pub(super) factory_info: Arc<RwLock<Arc<FactoryInfo>>>,
    pub(super) status_collector: StatusCollector,
    pub(super) wallet: Wallet,
    pub(super) config: Arc<DeploymentConfig>,
    pub(super) gas_wallet: Arc<Mutex<Wallet>>,
    pub(super) index: usize,
}

impl Trader {
    pub(super) fn start(self) {
        tokio::task::spawn(go(self));
    }
}

async fn go(worker: Trader) {
    loop {
        if let Err(e) = single(&worker).await {
            log::error!("Error running trader (#{}) process: {e:?}", worker.index);
        }
    }
}

async fn single(worker: &Trader) -> Result<()> {
    // Sleep a random amount of time between actions
    let secs = rand::thread_rng().gen_range(120..=1200);
    tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;

    let factory_info = worker.factory_info.read().clone();
    for market_addr in factory_info.markets.values() {
        single_market(worker, *market_addr, factory_info.faucet).await?;
    }
    Ok(())
}

async fn single_market(worker: &Trader, market_addr: Address, faucet: Address) -> Result<()> {
    let market = MarketContract::new(worker.cosmos.make_contract(market_addr));
    let status = market.status().await?;

    // Make sure we always have at least 50,000 tokens
    let balance = market
        .get_collateral_balance(&status, *worker.wallet.get_address())
        .await?;
    let cw20 = match &status.collateral {
        msg::token::Token::Cw20 {
            addr,
            decimal_places: _,
        } => addr.as_str().parse()?,
        msg::token::Token::Native { .. } => anyhow::bail!("Native not supported"),
    };
    if balance < "50000".parse().unwrap() {
        worker
            .config
            .wallet_manager
            .mint(
                worker.cosmos.clone(),
                *worker.wallet.get_address(),
                "200000".parse().unwrap(),
                &status,
                cw20,
                faucet,
            )
            .await?;
    }

    worker
        .status_collector
        .ensure_gas(
            worker.cosmos.clone(),
            *worker.wallet.get_address(),
            worker.config.min_gas.trader,
            worker.gas_wallet.clone(),
        )
        .await?;

    // Always close positions if utilization ratio is too high, otherwise randomly decide to open or close.
    let util_ratio = status
        .liquidity
        .locked
        .into_decimal256()
        .checked_div(status.liquidity.total_collateral().into_decimal256())
        .ok();
    let should_close = match util_ratio {
        Some(util_ratio) if util_ratio > "0.95".parse().unwrap() => true,
        _ => rand::thread_rng().gen_bool(0.5),
    };

    if should_close {
        if let Some(pos) = market
            .get_first_position(*worker.wallet.get_address())
            .await?
        {
            log::info!("Closing position {}", pos);
            market.close_position(&worker.wallet, pos).await?;
        }
    } else {
        let denominator = (status.long_notional + status.short_notional).into_decimal256();
        let short_likelihood = if denominator.is_zero() {
            0.5
        } else {
            (status.long_notional.into_decimal256() / denominator)
                .to_string()
                .parse()?
        };
        let direction = if rand::thread_rng().gen_bool(short_likelihood) {
            DirectionToBase::Short
        } else {
            DirectionToBase::Long
        };
        let deposit = Collateral::from(rand::thread_rng().gen_range(10..=400u64));
        let leverage = rand::thread_rng().gen_range(4..=30);
        log::info!("Opening new position: {deposit} {direction:?} {leverage}x");
        market
            .open_position(
                &worker.wallet,
                &status,
                deposit,
                direction,
                leverage.to_string().parse()?,
                "3".parse()?,
            )
            .await?;
    }
    Ok(())
}
