use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, Wallet};
use msg::prelude::*;
use perps_exes::contracts::MarketContract;
use rand::Rng;

use crate::{
    app::trader::EnsureCollateral,
    config::BotConfigTestnet,
    util::markets::Market,
    wallet_manager::ManagedWallet,
    watcher::{TaskLabel, WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, App, AppBuilder};

impl AppBuilder {
    pub(super) fn track_balance(&mut self) -> Result<()> {
        self.watch_periodic(TaskLabel::TrackBalance, TrackBalance)
    }
}

#[derive(Clone)]
struct TrackBalance;

#[async_trait]
impl WatchedTaskPerMarket for TrackBalance {
    async fn run_single_market(
        &mut self,
        _app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        check_balance_single(&market.market)
            .await
            .map(|()| WatchedTaskOutput {
                skip_delay: false,
                message: "Market is in balance".to_owned(),
            })
    }
}

async fn check_balance_single(market: &MarketContract) -> Result<()> {
    let status = market.status().await?;
    let net_notional = status.long_notional.into_number() - status.short_notional.into_number();
    let instant = net_notional / status.config.delta_neutrality_fee_sensitivity.into_signed();
    let instant_abs = instant.abs_unsigned();
    if instant_abs <= status.config.delta_neutrality_fee_cap.raw() {
        Ok(())
    } else if instant.is_negative() {
        Err(anyhow!("Protocol is too heavily short, need more longs"))
    } else {
        Err(anyhow!("Protocol is too heavily long, need more shorts"))
    }
}

struct Balance {
    app: Arc<App>,
    wallet: Wallet,
    testnet: Arc<BotConfigTestnet>,
}

impl AppBuilder {
    pub(super) fn start_balance(&mut self, testnet: Arc<BotConfigTestnet>) -> Result<()> {
        if testnet.balance {
            let balance = Balance {
                app: self.app.clone(),
                wallet: self.get_track_wallet(&testnet, ManagedWallet::Balance)?,
                testnet,
            };
            self.watch_periodic(TaskLabel::Balance, balance)?;
        }
        Ok(())
    }
}

#[async_trait]
impl WatchedTaskPerMarket for Balance {
    async fn run_single_market(
        &mut self,
        _app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        single_market(self, market, self.testnet.faucet).await
    }
}

async fn single_market(
    worker: &Balance,
    market: &Market,
    faucet: Address,
) -> Result<WatchedTaskOutput> {
    let market_id = &market.market_id;
    let market = &market.market;
    let status = market.status().await?;
    let net_notional = status.long_notional.into_number() - status.short_notional.into_number();
    let instant = net_notional / status.config.delta_neutrality_fee_sensitivity.into_signed();
    let instant_abs = instant.abs_unsigned();

    // Ensure the protocol stays within a 1/3 portion of the cap
    if instant_abs * Decimal256::from_str("3").unwrap()
        <= status.config.delta_neutrality_fee_cap.raw()
    {
        return Ok(WatchedTaskOutput {
            skip_delay: false,
            message: "Protocol is within 1/3 of the cap".to_owned(),
        });
    }

    let direction = if instant.is_negative() {
        DirectionToBase::Long
    } else {
        DirectionToBase::Short
    };

    // Check if we have a position to close.
    if let Some(pos) = market.get_first_position(*worker.wallet.address()).await? {
        let pos = market.query_position(pos).await?;
        if pos.direction_to_base != direction {
            log::info!(
                "Balancing {market_id} by closing {:?} position {}",
                pos.direction_to_base,
                pos.id
            );
            market.close_position(&worker.wallet, pos.id).await?;
            return Ok(WatchedTaskOutput {
                skip_delay: true,
                message: "Closed a position".to_owned(),
            });
        }
    }

    // If utilization ratio is too high, back off
    if status.liquidity.locked.into_decimal256()
        / (status.liquidity.total_collateral()).into_decimal256()
        > "0.99".parse().unwrap()
    {
        anyhow::bail!("Cannot balance {market_id}, utilization ratio is too high");
    }

    // Calculate the maximum deposit based on unlocked liquidity available
    //
    // Divide by 1.5 because of 2x leverage and 0.5x max gains, plus a small
    // buffer so we don't use up too much liquidity.
    let max_available_liquidity =
        status.liquidity.unlocked.into_decimal256() / Decimal256::from_str("1.5").unwrap();

    let price = market.current_price().await?;

    let leverage = LeverageToBase::from_str("2").unwrap();

    // We need to divide the desired notional impact by the leverage we'll be using.
    // We calculate this "leverage divider" by converting our actual leverage value
    // into notional leverage and then getting its absolute value.
    let leverage_divider = leverage
        .into_signed(direction)
        .into_notional(market_id.get_market_type())
        .into_number()
        .abs_unsigned();

    // Introduce a randomization factor as well
    let random_multiplier = {
        let mut rng = rand::thread_rng();
        let percent = rng.gen_range(80..=100u32);
        Decimal256::from_ratio(percent, 100u32)
    };

    let collateral_for_balance = price
        .notional_to_collateral(Notional::from_decimal256(net_notional.abs_unsigned()))
        .into_decimal256()
        .checked_div(leverage_divider)?
        .checked_mul(random_multiplier)?;
    log::info!("collateral_for_balance: {}", collateral_for_balance);

    let needed_collateral = Collateral::from_decimal256(
        collateral_for_balance
            .min(max_available_liquidity)
            // arbitrary limit, we don't want to open positions that are too large
            .min(
                match status.market_id.get_collateral() {
                    "ETH" => "300",
                    _ => "10000",
                }
                .parse()
                .unwrap(),
            ),
    );

    log::info!(
            "Need to balance {market_id} by opening a {direction:?} with {needed_collateral} deposit. Net notional {net_notional}."
        );

    // Make sure we always have enough collateral
    EnsureCollateral {
        market: &market,
        wallet: &worker.wallet,
        status: &status,
        testnet: &worker.testnet,
        cosmos: &worker.app.cosmos,
        min: needed_collateral,
        to_mint: needed_collateral,
        faucet,
    }
    .run()
    .await?;

    let needed_collateral = NonZero::new(needed_collateral).context("needed_collateral was 0")?;

    market
        .open_position(
            &worker.wallet,
            &status,
            needed_collateral,
            direction,
            leverage,
            "0.5".parse().unwrap(),
            None,
            None,
            None,
        )
        .await?;

    Ok(WatchedTaskOutput {
        skip_delay: true,
        message: "Opened a new position to balance the protocol".to_owned(),
    })
}
