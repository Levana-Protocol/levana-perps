use std::sync::Arc;

use anyhow::Result;
use cosmos::{Address, Cosmos, HasAddress, Wallet};
use msg::{contracts::market::entry::StatusResp, prelude::*};
use parking_lot::RwLock;
use perps_exes::config::DeploymentConfig;
use tokio::sync::Mutex;

use crate::market_contract::MarketContract;

use super::{factory::FactoryInfo, status_collector::StatusCollector};

pub(super) struct Utilization {
    pub(super) cosmos: Cosmos,
    pub(super) factory_info: Arc<RwLock<Arc<FactoryInfo>>>,
    pub(super) status_collector: StatusCollector,
    pub(super) wallet: Wallet,
    pub(super) config: Arc<DeploymentConfig>,
    pub(super) gas_wallet: Arc<Mutex<Wallet>>,
}

impl Utilization {
    pub(super) fn start(self) {
        tokio::task::spawn(go(self));
    }
}

async fn go(worker: Utilization) {
    loop {
        if let Err(e) = single(&worker).await {
            log::error!("Error running utilization process: {e:?}");
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(120)).await;
    }
}

async fn single(worker: &Utilization) -> Result<()> {
    let factory_info = worker.factory_info.read().clone();
    for market_addr in factory_info.markets.values() {
        single_market(worker, *market_addr, factory_info.faucet).await?;
    }
    Ok(())
}

async fn single_market(worker: &Utilization, market_addr: Address, faucet: Address) -> Result<()> {
    let market = MarketContract::new(worker.cosmos.make_contract(market_addr));
    let status = market.status().await?;
    let total = status.liquidity.total_collateral();
    let min_locked = total.checked_mul_dec(status.config.target_utilization.raw())?;
    let max_locked = total.checked_mul_dec("0.95".parse().unwrap())?;
    if min_locked >= status.liquidity.locked {
        log::info!("Plenty of locked");
        return Ok(());
    } else if status.liquidity.locked >= max_locked {
        if let Some(pos) = market
            .get_first_position(*worker.wallet.get_address())
            .await?
        {
            log::info!("Locked too much liquidity, time to unlock some. Closing {pos}");
            market.close_position(&worker.wallet, pos).await?;
        } else {
            log::info!("Too much locked liquidity, but I don't have any positions to close");
        }
        return Ok(());
    };

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
    if balance < "20000".parse().unwrap() {
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
            worker.config.min_gas.utilization,
            worker.gas_wallet.clone(),
        )
        .await?;

    let res1 = open(worker, &status, &market, DirectionToBase::Long).await;
    let res2 = open(worker, &status, &market, DirectionToBase::Short).await;

    match (res1, res2) {
        (Err(e1), Err(e2)) => Err(anyhow::anyhow!(
            "Long and short both failed\n{e1:?}\n{e2:?}"
        )),
        _ => Ok(()),
    }
}

async fn open(
    worker: &Utilization,
    status: &StatusResp,
    market: &MarketContract,
    direction: DirectionToBase,
) -> Result<()> {
    market
        .open_position(
            &worker.wallet,
            status,
            "500".parse().unwrap(),
            direction,
            "8".parse().unwrap(),
            "3".parse().unwrap(),
        )
        .await
}
