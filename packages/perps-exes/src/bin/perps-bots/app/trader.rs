use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Cosmos, HasAddress, Wallet};
use perps_exes::{config::TraderConfig, prelude::*};
use perpswap::contracts::market::entry::StatusResp;
use rand::Rng;

use crate::{
    config::BotConfigTestnet,
    util::markets::Market,
    wallet_manager::ManagedWallet,
    watcher::{WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, App, AppBuilder};

pub(super) struct Trader {
    pub(super) app: Arc<App>,
    pub(super) wallet: Wallet,
    config: TraderConfig,
    testnet: Arc<BotConfigTestnet>,
}

impl AppBuilder {
    pub(super) fn start_traders(&mut self, testnet: Arc<BotConfigTestnet>) -> Result<()> {
        if let Some((traders, config)) = testnet.trader_config {
            for index in 1..=traders {
                let wallet = self.get_track_wallet(ManagedWallet::Trader(index))?;
                let trader = Trader {
                    app: self.app.clone(),
                    wallet,
                    config,
                    testnet: testnet.clone(),
                };
                self.watch_periodic(crate::watcher::TaskLabel::Trader { index }, trader)?;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl WatchedTaskPerMarket for Trader {
    async fn run_single_market(
        &mut self,
        _app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        single_market(self, &market.market, self.testnet.faucet).await
    }
}

pub(crate) struct EnsureCollateral<'a> {
    pub(crate) market: &'a MarketContract,
    pub(crate) wallet: &'a Wallet,
    pub(crate) status: &'a StatusResp,
    pub(crate) testnet: &'a BotConfigTestnet,
    pub(crate) cosmos: &'a Cosmos,
    pub(crate) min: Collateral,
    pub(crate) to_mint: Collateral,
    pub(crate) faucet: Address,
}

impl EnsureCollateral<'_> {
    pub(crate) async fn run(&self) -> Result<()> {
        let balance = self
            .market
            .get_collateral_balance(self.status, self.wallet.get_address())
            .await?;
        let cw20 = match &self.status.collateral {
            perpswap::token::Token::Cw20 {
                addr,
                decimal_places: _,
            } => addr.as_str().parse()?,
            perpswap::token::Token::Native { .. } => anyhow::bail!("Native not supported"),
        };
        if balance < self.min {
            self.testnet
                .wallet_manager
                .mint(
                    self.cosmos.clone(),
                    self.wallet.get_address(),
                    self.to_mint,
                    self.status,
                    cw20,
                    self.faucet,
                )
                .await?;
        }
        Ok(())
    }
}

async fn single_market(
    worker: &Trader,
    market: &MarketContract,
    faucet: Address,
) -> Result<WatchedTaskOutput> {
    let status = market.status().await?;

    // Make sure we always have at least 50,000 tokens
    EnsureCollateral {
        market,
        wallet: &worker.wallet,
        status: &status,
        testnet: &worker.testnet,
        cosmos: &worker.app.cosmos,
        min: "50000".parse().unwrap(),
        to_mint: "200000".parse().unwrap(),
        faucet,
    }
    .run()
    .await?;

    let total = status.liquidity.total_collateral()?;
    if total.is_zero() {
        return Ok(WatchedTaskOutput::new("No liquidity available".to_owned()));
    }
    let util_ratio = status
        .liquidity
        .locked
        .into_decimal256()
        .checked_div(status.liquidity.total_collateral()?.into_decimal256())?;

    enum Action {
        Open,
        CloseSingle,
        CloseMany,
    }

    let max_util = status
        .config
        .target_utilization
        .raw()
        .checked_add_signed(worker.config.max_util_delta)?;
    let action = if util_ratio > max_util {
        Action::CloseMany
    } else if status.borrow_fee < worker.config.min_borrow_fee {
        Action::Open
    } else if status.borrow_fee > worker.config.max_borrow_fee {
        Action::CloseMany
    } else {
        let to_u32 = |x: Decimal256| -> Result<u32, _> {
            (x * Decimal256::from_str("1000").unwrap())
                .to_uint_floor()
                .to_string()
                .parse()
        };
        let min = to_u32(worker.config.min_borrow_fee)?;
        let max = to_u32(worker.config.max_borrow_fee)?;
        let cutoff = to_u32(status.borrow_fee)?;
        let rand = rand::thread_rng().gen_range(min..=max);
        let should_close = cutoff > rand;
        if should_close {
            Action::CloseSingle
        } else {
            Action::Open
        }
    };

    let message = match action {
        Action::CloseSingle => {
            if let Some(pos) = market
                .get_first_position(worker.wallet.get_address())
                .await?
            {
                market.close_position(&worker.wallet, pos).await?;
                format!("Closed position {}", pos)
            } else {
                format!(
                    "Wanted to close, but wallet {} has no open positions",
                    worker.wallet
                )
            }
        }
        Action::CloseMany => {
            let positions = market
                .get_some_positions(worker.wallet.get_address(), Some(20))
                .await?;
            if positions.is_empty() {
                format!(
                    "Wanted to close many, but wallet {} has no open positions",
                    worker.wallet
                )
            } else {
                let message = format!("Closed positions: {positions:?}");
                market.close_positions(&worker.wallet, positions).await?;
                message
            }
        }
        Action::Open => {
            let denominator = (status.long_notional + status.short_notional)?.into_decimal256();
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
            let deposit =
                NonZero::new(Collateral::from(rand::thread_rng().gen_range(10..=400u64))).unwrap();
            let leverage = rand::thread_rng().gen_range(2..=8);
            market
                .open_position(
                    &worker.wallet,
                    &status,
                    deposit,
                    direction,
                    leverage.to_string().parse()?,
                    None,
                    None,
                    None,
                )
                .await.with_context(|| format!("Opening position with {deposit} deposit, {direction:?}, {leverage}x leverage"))?;
            format!("Opened new position: {deposit} {direction:?} {leverage}x")
        }
    };
    Ok(WatchedTaskOutput::new(message))
}
