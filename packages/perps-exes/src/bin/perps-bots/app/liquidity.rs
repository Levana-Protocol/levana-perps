use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Wallet};
use cosmwasm_std::Fraction;
use perps_exes::{config::LiquidityConfig, prelude::*};

use crate::{
    config::BotConfigTestnet,
    util::markets::Market,
    wallet_manager::ManagedWallet,
    watcher::{WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, App, AppBuilder};

pub(super) struct Liquidity {
    pub(super) app: Arc<App>,
    liquidity_config: LiquidityConfig,
    pub(super) wallet: Wallet,
    testnet: Arc<BotConfigTestnet>,
}

impl AppBuilder {
    pub(super) fn start_liquidity(&mut self, testnet: Arc<BotConfigTestnet>) -> Result<()> {
        if let Some(liquidity_config) = &testnet.liquidity_config {
            let liquidity = Liquidity {
                app: self.app.clone(),
                liquidity_config: liquidity_config.clone(),
                wallet: self.get_track_wallet(&testnet, ManagedWallet::Liquidity)?,
                testnet,
            };
            self.watch_periodic(crate::watcher::TaskLabel::Liquidity, liquidity)?;
        }
        Ok(())
    }
}

#[async_trait]
impl WatchedTaskPerMarket for Liquidity {
    async fn run_single_market(
        &mut self,
        _app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        single_market(self, &market.market_id, &market.market, self.testnet.faucet).await
    }
}

enum Action {
    Deposit(Collateral),
    Withdraw(Collateral),
    None,
}

async fn single_market(
    worker: &Liquidity,
    market_id: &MarketId,
    market: &MarketContract,
    faucet: Address,
) -> Result<WatchedTaskOutput> {
    let status = market.status().await?;
    let total = status.liquidity.total_collateral();
    let bounds = worker
        .liquidity_config
        .markets
        .get(market_id)
        .with_context(|| format!("No bounds available for market {market_id}"))?;
    let min_liquidity = bounds.min;
    let max_liquidity = bounds.max;
    let util = if total.is_zero() {
        Decimal256::zero()
    } else {
        status.liquidity.locked.into_decimal256() / total.into_decimal256()
    };
    let high_util = status
        .config
        .target_utilization
        .raw()
        .checked_add_signed(worker.liquidity_config.max_util_delta)?;
    let low_util = status
        .config
        .target_utilization
        .raw()
        .checked_add_signed(worker.liquidity_config.min_util_delta)?;
    let target_util = status
        .config
        .target_utilization
        .raw()
        .checked_add_signed(worker.liquidity_config.target_util_delta)?;
    let target_liquidity = status
        .liquidity
        .locked
        .checked_mul_dec(target_util.inv().context("Cannot invert target util")?)?;

    let lp_info = market.lp_info(&worker.wallet).await.with_context(|| {
        format!(
            "Error loading LP info for {} in market {market_id}",
            worker.wallet
        )
    })?;

    let action = if let Some(want_to_remove) = lp_info
        .lp_collateral
        .checked_sub(max_liquidity)
        .ok()
        .and_then(NonZero::new)
    {
        Action::Withdraw(want_to_remove.raw().min(status.liquidity.unlocked))
    } else if let Some(missing) = min_liquidity
        .checked_sub(lp_info.lp_collateral)
        .ok()
        .and_then(NonZero::new)
    {
        Action::Deposit(missing.raw() + Collateral::one())
    } else if util < low_util {
        Action::Withdraw(total.checked_sub(target_liquidity)?)
    } else if util > high_util {
        Action::Deposit(target_liquidity - total)
    } else {
        Action::None
    };

    Ok(match action {
        Action::Deposit(to_deposit) => {
            let max_deposit = max_liquidity.checked_sub(lp_info.lp_collateral)?;
            let to_deposit = to_deposit.min(max_deposit);
            if to_deposit < Collateral::one() {
                return Ok(WatchedTaskOutput {
                    skip_delay: false,
                    message: "Too little collateral to warrant a deposit, skipping".to_owned(),
                });
            }
            let cw20 = match &status.collateral {
                msg::token::Token::Cw20 {
                    addr,
                    decimal_places: _,
                } => addr.as_str().parse()?,
                msg::token::Token::Native { .. } => anyhow::bail!("No support for native coins"),
            };
            worker
                .testnet
                .wallet_manager
                .mint(
                    worker.app.cosmos.clone(),
                    *worker.wallet.address(),
                    to_deposit,
                    &status,
                    cw20,
                    faucet,
                )
                .await?;
            let to_deposit = NonZero::new(to_deposit).context("to_deposit is 0")?;
            market.deposit(&worker.wallet, &status, to_deposit).await?;
            WatchedTaskOutput {
                skip_delay: true,
                message: format!("Deposited {to_deposit} liquidity"),
            }
        }
        Action::Withdraw(to_withdraw) => {
            let max_withdrawal = lp_info.lp_collateral.checked_sub(min_liquidity)?;
            let to_withdraw = to_withdraw.min(max_withdrawal);
            assert!(to_withdraw <= lp_info.lp_collateral);
            if to_withdraw < Collateral::one() {
                WatchedTaskOutput {
                    skip_delay: false,
                    message: "Won't withdraw less than 1 liquidity".to_owned(),
                }
            } else {
                let lp_tokens = to_withdraw.into_decimal256()
                    * status.liquidity.total_tokens().into_decimal256()
                    / status.liquidity.total_collateral().into_decimal256();
                let lp_tokens = NonZero::new(LpToken::from_decimal256(lp_tokens))
                    .context("Somehow got 0 to withdraw")?;
                market.withdraw(&worker.wallet, lp_tokens).await?;
                WatchedTaskOutput {
                    skip_delay: true,
                    message: format!("Withdrew {to_withdraw} collateral"),
                }
            }
        }
        Action::None => WatchedTaskOutput {
            skip_delay: false,
            message: "No actions needed".to_owned(),
        },
    })
}
