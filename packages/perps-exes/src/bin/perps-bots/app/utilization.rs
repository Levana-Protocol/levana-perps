use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{Address, HasAddress, Wallet};
use perps_exes::{config::UtilizationConfig, prelude::*};

use crate::{
    config::BotConfigTestnet,
    util::markets::Market,
    wallet_manager::ManagedWallet,
    watcher::{WatchedTaskOutput, WatchedTaskPerMarket},
};

use super::{factory::FactoryInfo, App, AppBuilder};

pub(super) struct Utilization {
    pub(super) app: Arc<App>,
    pub(super) wallet: Wallet,
    config: UtilizationConfig,
    testnet: Arc<BotConfigTestnet>,
}

impl AppBuilder {
    pub(super) fn start_utilization(&mut self, testnet: Arc<BotConfigTestnet>) -> Result<()> {
        if let Some(config) = testnet.utilization_config {
            let util = Utilization {
                app: self.app.clone(),
                wallet: self.get_track_wallet(&testnet, ManagedWallet::Utilization)?,
                config,
                testnet,
            };
            self.watch_periodic(crate::watcher::TaskLabel::Utilization, util)?;
        }
        Ok(())
    }
}

#[async_trait]
impl WatchedTaskPerMarket for Utilization {
    async fn run_single_market(
        &mut self,
        _app: &App,
        _factory: &FactoryInfo,
        market: &Market,
    ) -> Result<WatchedTaskOutput> {
        single_market(self, &market.market, self.testnet.faucet).await
    }
}

async fn single_market(
    worker: &Utilization,
    market: &MarketContract,
    faucet: Address,
) -> Result<WatchedTaskOutput> {
    let status = market.status().await?;

    if status.is_stale() {
        return Ok(WatchedTaskOutput {
            skip_delay: false,
            message: "Protocol is currently stale, skipping".to_owned(),
        });
    }

    let total = status.liquidity.total_collateral();
    if total.is_zero() {
        return Ok(WatchedTaskOutput {
            skip_delay: false,
            message: "No deposited collateral".to_owned(),
        });
    }
    let util = status
        .liquidity
        .locked
        .into_decimal256()
        .checked_div(total.into_decimal256())?;
    let max_util = status
        .config
        .target_utilization
        .raw()
        .checked_add_signed(worker.config.max_util_delta)?;
    let min_util = status
        .config
        .target_utilization
        .raw()
        .checked_add_signed(worker.config.min_util_delta)?;

    if util > max_util {
        let positions = market
            .get_some_positions(worker.wallet.get_address(), Some(20))
            .await?;
        if positions.is_empty() {
            Ok(WatchedTaskOutput {
                skip_delay: false,
                message: "High utilization ratio, but I don't have any positions to close"
                    .to_owned(),
            })
        } else {
            let message = format!(
                "High utilization ratio, time to unlock some liquidity. Closing {positions:?}"
            );
            market.close_positions(&worker.wallet, positions).await?;
            Ok(WatchedTaskOutput {
                skip_delay: true,
                message,
            })
        }
    } else if util < min_util {
        tracing::info!("Low utilization ratio, opening positions.");

        let balance = market
            .get_collateral_balance(&status, worker.wallet.get_address())
            .await?;
        let cw20 = match &status.collateral {
            msg::token::Token::Cw20 {
                addr,
                decimal_places: _,
            } => addr.as_str().parse()?,
            msg::token::Token::Native { .. } => anyhow::bail!("Native not supported"),
        };

        // Open unpopular positions
        let direction = if status.long_notional > status.short_notional {
            DirectionToBase::Short
        } else {
            DirectionToBase::Long
        };
        if balance < "20000".parse().unwrap() {
            worker
                .testnet
                .wallet_manager
                .mint(
                    worker.app.cosmos.clone(),
                    worker.wallet.get_address(),
                    "200000".parse().unwrap(),
                    &status,
                    cw20,
                    faucet,
                )
                .await?;
        }

        // Maybe make these config values?
        let leverage: LeverageToBase = "8".parse().unwrap();
        let max_gains: MaxGainsInQuote = match (status.market_type, direction) {
            (MarketType::CollateralIsBase, DirectionToBase::Long) => MaxGainsInQuote::PosInfinity,
            _ => "2".parse().unwrap(),
        };

        // Determine how large a position we would need to open to hit the midpoint of min and max utilization
        let min_util = status
            .config
            .target_utilization
            .raw()
            .checked_add_signed(worker.config.min_util_delta)?;
        let max_util = status
            .config
            .target_utilization
            .raw()
            .checked_add_signed(worker.config.max_util_delta)?;
        let mid_util = min_util
            .checked_add(max_util)?
            .checked_div("2".parse().unwrap())?;
        let extra_util = mid_util.checked_sub(util)?;
        let desired_counter_collateral = NonZero::new(total.checked_mul_dec(extra_util)?)
            .context("Calculated a 0 desired_counter_collateral")?;
        let desired_deposit_collateral = counter_to_deposit(
            status.market_type,
            desired_counter_collateral,
            leverage,
            max_gains,
            direction,
        )?;
        let price = market.current_price().await?;

        // Farthest from neutral the protocol is allowed to go
        let notional_high_cap = Notional::from_decimal256(
            status.config.delta_neutrality_fee_cap.raw()
                * status.config.delta_neutrality_fee_sensitivity.raw(),
        );
        // Since we're opening an unpopular position: add the high cap with the
        // absolute value of net notional.
        let largest_notional_size_abs = notional_high_cap
            + match direction {
                DirectionToBase::Long => status.short_notional - status.long_notional,
                DirectionToBase::Short => status.long_notional - status.short_notional,
            };
        let largest_deposit_collateral = price
            .notional_to_collateral(Notional::from_decimal256(
                largest_notional_size_abs.into_decimal256().checked_div(
                    leverage
                        .into_signed(direction)
                        .into_notional(status.market_type)
                        .into_number()
                        .abs_unsigned(),
                )?,
            ))
            // Avoid getting too close to the limit
            .checked_mul_dec("0.95".parse().unwrap())?;

        let deposit_collateral =
            NonZero::new(largest_deposit_collateral.min(desired_deposit_collateral.raw()))
                .context("deposit_collateral is 0")?;

        let desc = format!("Opening a {direction:?} {leverage}x with {max_gains} max gains and {deposit_collateral} collateral");
        let res = market
            .open_position(
                &worker.wallet,
                &status,
                deposit_collateral,
                direction,
                leverage,
                max_gains,
                None,
                None,
                None,
            )
            .await
            .with_context(|| desc.clone())?;

        Ok(WatchedTaskOutput {
            skip_delay: true,
            message: format!("Success! {desc} {}", res.txhash),
        })
    } else {
        Ok(WatchedTaskOutput {
            skip_delay: false,
            message: "No work to do".to_owned(),
        })
    }
}

/// Convert a counter collateral amount into a deposit collateral amount.
fn counter_to_deposit(
    market_type: MarketType,
    counter: NonZero<Collateral>,
    leverage: LeverageToBase,
    max_gains: MaxGainsInQuote,
    direction: DirectionToBase,
) -> Result<NonZero<Collateral>> {
    Ok(match market_type {
        MarketType::CollateralIsQuote => match max_gains {
            MaxGainsInQuote::Finite(max_gains_in_collateral) => {
                counter.checked_mul_non_zero(max_gains_in_collateral.inverse())?
            }
            MaxGainsInQuote::PosInfinity => {
                anyhow::bail!("Collateral-is-quote markets do not support infinite max gains")
            }
        },
        MarketType::CollateralIsBase => match max_gains {
            MaxGainsInQuote::PosInfinity => {
                if direction == DirectionToBase::Short {
                    anyhow::bail!("Infinite max gains are only allowed on Long positions");
                }

                let leverage_notional = leverage.into_signed(direction).into_notional(market_type);

                NonZero::new(Collateral::from_decimal256(
                    counter
                        .into_decimal256()
                        .checked_div(leverage_notional.into_number().abs_unsigned())?,
                ))
                .context("counter_to_deposit: got a 0 deposit collateral")?
            }
            MaxGainsInQuote::Finite(max_gains_in_notional) => {
                let leverage_notional = leverage.into_signed(direction).into_notional(market_type);
                let max_gains_multiple = Number::ONE
                    - (max_gains_in_notional.into_number() + Number::ONE)
                        .checked_div(leverage_notional.into_number().abs())?;

                if max_gains_multiple.approx_lt_relaxed(Number::ZERO) {
                    return Err(MarketError::MaxGainsTooLarge {}.into());
                }

                let deposit = (counter.into_number() * max_gains_multiple)
                    .checked_div(max_gains_in_notional.into_number())?;

                NonZero::<Collateral>::try_from_number(deposit).with_context(|| {
                    format!("Calculated an invalid deposit collateral: {deposit}")
                })?
            }
        },
    })
}
