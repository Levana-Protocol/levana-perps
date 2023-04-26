use std::sync::Arc;

use anyhow::Result;
use cosmos::{Address, Cosmos, HasAddress, Wallet};
use msg::prelude::*;
use parking_lot::RwLock;
use perps_exes::config::DeploymentConfig;
use tokio::sync::Mutex;

use crate::market_contract::MarketContract;

use super::{factory::FactoryInfo, status_collector::StatusCollector};

pub(super) struct Liquidity {
    pub(super) cosmos: Cosmos,
    pub(super) factory_info: Arc<RwLock<Arc<FactoryInfo>>>,
    pub(super) status_collector: StatusCollector,
    pub(super) wallet: Wallet,
    pub(super) config: Arc<DeploymentConfig>,
    pub(super) gas_wallet: Arc<Mutex<Wallet>>,
}

impl Liquidity {
    pub(super) fn start(self) {
        tokio::task::spawn(go(self));
    }
}

async fn go(worker: Liquidity) {
    loop {
        if let Err(e) = single(&worker).await {
            log::error!("Error running liquidity process: {e:?}");
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(120)).await;
    }
}

async fn single(worker: &Liquidity) -> Result<()> {
    let factory_info = worker.factory_info.read().clone();
    for (market_id, market_addr) in &factory_info.markets {
        single_market(worker, market_id, *market_addr, factory_info.faucet).await?;
    }
    Ok(())
}

fn min_liquidity(market_id: &MarketId) -> Result<Collateral> {
    match market_id.get_collateral() {
        "ATOM" => "10000",
        "BTC" => "1000",
        "USDC" => "100000",
        x => anyhow::bail!("Unknown collateral: {x}"),
    }
    .parse()
}

fn max_liquidity(market_id: &MarketId) -> Result<Collateral> {
    match market_id.get_collateral() {
        "ATOM" => "10000000",
        "BTC" => "1000000",
        "USDC" => "100000000",
        x => anyhow::bail!("Unknown collateral: {x}"),
    }
    .parse()
}

enum Action {
    Deposit(Collateral),
    Withdraw(Collateral),
    None,
}

async fn single_market(
    worker: &Liquidity,
    market_id: &MarketId,
    market_addr: Address,
    faucet: Address,
) -> Result<()> {
    let market = MarketContract::new(worker.cosmos.make_contract(market_addr));
    let status = market.status().await?;
    let total = status.liquidity.total_collateral();
    let min_liquidity = min_liquidity(market_id)?;
    let max_liquidity = max_liquidity(market_id)?;
    let util = status.liquidity.locked.into_decimal256() / total.into_decimal256();
    let high_util = status
        .config
        .target_utilization
        .raw()
        .checked_mul("1.05".parse().unwrap())?;
    let low_util = status
        .config
        .target_utilization
        .raw()
        .checked_mul("0.95".parse().unwrap())?;

    let lp_info = market.lp_info(&worker.wallet).await?;

    let action = if let Ok(want_to_remove) = lp_info.lp_collateral.checked_sub(max_liquidity) {
        Action::Withdraw(want_to_remove.min(status.liquidity.unlocked))
    } else if let Ok(missing) = min_liquidity.checked_sub(total) {
        Action::Deposit(missing + Collateral::one())
    } else if util < low_util {
        Action::Withdraw(
            status
                .liquidity
                .unlocked
                .checked_mul_dec("0.5".parse().unwrap())?,
        )
    } else if util > high_util {
        Action::Deposit(Collateral::from_decimal256(
            status.liquidity.locked.into_decimal256() / status.config.target_utilization.raw()
                - total.into_decimal256(),
        ))
    } else {
        Action::None
    };

    match action {
        Action::Deposit(to_deposit) => {
            log::info!("Going to deposit {to_deposit} into {market_id} ({market_addr})");
            let cw20 = match &status.collateral {
                msg::token::Token::Cw20 {
                    addr,
                    decimal_places: _,
                } => addr.as_str().parse()?,
                msg::token::Token::Native { .. } => anyhow::bail!("No support for native coins"),
            };
            worker
                .status_collector
                .ensure_gas(
                    worker.cosmos.clone(),
                    *worker.wallet.get_address(),
                    worker.config.min_gas.liquidity,
                    worker.gas_wallet.clone(),
                )
                .await?;
            worker
                .config
                .wallet_manager
                .mint(
                    worker.cosmos.clone(),
                    *worker.wallet.address(),
                    to_deposit,
                    &status,
                    cw20,
                    faucet,
                )
                .await?;
            market.deposit(&worker.wallet, &status, to_deposit).await?;
        }
        Action::Withdraw(to_withdraw) => {
            let to_withdraw = to_withdraw.min(lp_info.lp_collateral);
            if to_withdraw.is_zero() {
                log::info!("Cannot withdraw 0 liquidity");
            } else {
                log::info!("Going to withdraw {to_withdraw} from {market_id} ({market_addr})");
                let lp_tokens = to_withdraw.into_decimal256()
                    * status.liquidity.total_tokens().into_decimal256()
                    / status.liquidity.total_collateral().into_decimal256();
                let lp_tokens = NonZero::new(LpToken::from_decimal256(lp_tokens))
                    .context("Somehow got 0 to withdraw")?;
                market.withdraw(&worker.wallet, lp_tokens).await?;
            }
        }
        Action::None => (),
    }

    Ok(())
}
