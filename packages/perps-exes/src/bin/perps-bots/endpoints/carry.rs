use std::sync::Arc;

use crate::app::App;
use anyhow::Result;
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use cosmos::Address;
use cosmwasm_std::Decimal256;
use perps_exes::PositionsInfo;
use perpswap::prelude::{MarketId, MarketType, UnsignedDecimal};

use super::RestApp;

pub(crate) async fn carry(
    State(rest_app): State<RestApp>,
    headers: HeaderMap,
    params: axum::extract::Query<CarryParams>,
) -> Response {
    match carry_inner(rest_app.app, headers, &params).await {
        Ok(res) => res,
        Err(err) => err.to_string().into_response(),
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct FundingConfig {
    pub funding_rate_max_annualized: f64,
    pub funding_rate_sensitivity: f64,
    pub delta_neutrality_fee_sensitivity: f64,
    pub delta_neutrality_fee_cap: f64,
}

#[derive(Clone, PartialEq, Debug)]
pub struct OpenInterestAndFunding {
    pub long_notional: f64,
    pub short_notional: f64,
    pub long_funding: f64,
    pub short_funding: f64,
}

impl OpenInterestAndFunding {
    pub fn new(long_notional: f64, short_notional: f64, config: FundingConfig) -> Self {
        let rf_per_annual_cap = config.funding_rate_max_annualized;

        let instant_net_open_interest = long_notional - short_notional;
        let instant_open_short = short_notional;
        let instant_open_long = long_notional;
        let funding_rate_sensitivity = config.funding_rate_sensitivity;

        let total_interest = instant_open_long + instant_open_short;
        let notional_high_cap =
            config.delta_neutrality_fee_sensitivity * config.delta_neutrality_fee_cap;
        let funding_rate_sensitivity_from_delta_neutrality =
            rf_per_annual_cap * total_interest / notional_high_cap;

        let effective_funding_rate_sensitivity =
            funding_rate_sensitivity.max(funding_rate_sensitivity_from_delta_neutrality);
        let rf_popular = || -> f64 {
            (effective_funding_rate_sensitivity
                * (instant_net_open_interest.abs() / (instant_open_long + instant_open_short)))
                .min(rf_per_annual_cap)
        };

        let rf_unpopular = || -> f64 {
            match instant_open_long.total_cmp(&instant_open_short) {
                std::cmp::Ordering::Greater => {
                    rf_popular() * instant_open_long / instant_open_short
                }
                std::cmp::Ordering::Less => rf_popular() * instant_open_short / instant_open_long,
                std::cmp::Ordering::Equal => 0f64,
            }
        };

        let (long_rate, short_rate) = if instant_open_long == 0f64 || instant_open_short == 0f64 {
            // When all on one side, popular side has no one to pay
            (0f64, 0f64)
        } else {
            match instant_open_long.total_cmp(&instant_open_short) {
                std::cmp::Ordering::Greater => (rf_popular(), -rf_unpopular()),
                std::cmp::Ordering::Less => (-rf_unpopular(), rf_popular()),
                std::cmp::Ordering::Equal => (0f64, 0f64),
            }
        };

        Self {
            long_notional,
            short_notional,
            long_funding: long_rate,
            short_funding: short_rate,
        }
    }
}

pub fn bin_search(mut lower: f64, mut upper: f64, check: impl Fn(f64) -> bool) -> f64 {
    let eps = 1e-6;

    assert!(lower <= upper);
    if check(upper) {
        lower = upper;
    }

    if !check(lower) {
        upper = lower;
    }

    while (upper - lower).abs() > eps {
        let mid = (upper + lower) * 0.5f64;
        if check(mid) {
            lower = mid;
        } else {
            upper = mid;
        }
    }

    (upper + lower) * 0.5f64
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct CarryParams {
    addr: Address,
    cc_funding_long_open: Option<f64>,
    cc_funding_long_close: Option<f64>,
    cc_funding_short_open: Option<f64>,
    cc_funding_short_close: Option<f64>,
    max_notional_size_usd: Option<f64>,
    target_leverage_to_notional: Option<f64>,
    market_id: Option<MarketId>,
}

pub(crate) async fn carry_inner(
    app: Arc<App>,
    _headers: HeaderMap,
    CarryParams {
        addr: cc_addr,
        cc_funding_long_open,
        cc_funding_long_close,
        cc_funding_short_open,
        cc_funding_short_close,
        max_notional_size_usd,
        target_leverage_to_notional,
        market_id,
    }: &CarryParams,
) -> Result<Response> {
    // Fill in defaults. We could do this with serde instead, but this is less total code.
    let cc_funding_long_open = cc_funding_long_open.unwrap_or(-0.3);
    let cc_funding_long_close = cc_funding_long_close.unwrap_or(-0.2);
    let cc_funding_short_open = cc_funding_short_open.unwrap_or(-0.15);
    let cc_funding_short_close = cc_funding_short_close.unwrap_or(-0.05);
    let max_notional_size_usd = max_notional_size_usd.unwrap_or(1e5);
    let target_leverage_to_notional = target_leverage_to_notional.unwrap_or(2.0);

    let mut res_str = "Cash & carry:\n".to_string();

    res_str += format!("cc_addr: {}\n", cc_addr).as_str();

    let factory = app.get_factory_info().await;
    let market_iter = factory.markets.iter().filter(|market| {
        market_id
            .as_ref()
            .map_or(true, |market_id| market.market_id == *market_id)
    });
    'market: for market in market_iter {
        let market = &market.market;
        let status = market.status().await?;
        let price_point = market.current_price().await?;
        let price_notional: f64 = price_point
            .price_notional
            .into_number()
            .to_string()
            .parse::<f64>()?;
        let price_collateral_usd: f64 = price_point
            .price_usd
            .into_number()
            .to_string()
            .parse::<f64>()?;

        res_str = res_str + "\n" + status.market_id.as_str() + "\n";

        let max_notional_size = max_notional_size_usd / price_notional / price_collateral_usd;

        let (collateral, notional) = match status.market_type {
            MarketType::CollateralIsQuote => (status.quote.as_str(), status.base.as_str()),
            MarketType::CollateralIsBase => (status.base.as_str(), status.quote.as_str()),
        };

        let positions: PositionsInfo = market.all_open_positions(cc_addr, None).await?;
        let mut total_notional_size = Decimal256::zero().into_signed();
        let mut has_long = false;
        let mut has_short = false;
        let positions_count = positions.info.len();
        for pos in positions.info {
            res_str += format!(
                "- Existing position with notional_size: {:?} {notional}\n",
                pos.notional_size
            )
            .as_str();
            total_notional_size = (total_notional_size + pos.notional_size.into_number())?;
            if pos.notional_size.is_negative() {
                has_short = true;
            } else {
                has_long = true;
            }
        }

        let total_notional_size: f64 = total_notional_size.to_string().parse::<f64>()?;

        let long_funding_base: f64 = status.long_funding.to_string().parse::<f64>()?;
        let short_funding_base: f64 = status.short_funding.to_string().parse::<f64>()?;
        let long_interest: f64 = status.long_notional.to_string().parse::<f64>()?;
        let short_interest: f64 = status.short_notional.to_string().parse::<f64>()?;
        let (long_funding_notional, short_funding_notional, long_interest, short_interest) =
            match status.market_type {
                MarketType::CollateralIsQuote => (
                    long_funding_base,
                    short_funding_base,
                    long_interest,
                    short_interest,
                ),
                MarketType::CollateralIsBase => (
                    short_funding_base,
                    long_funding_base,
                    short_interest,
                    long_interest,
                ),
            };

        let (long_str, short_str) = match status.market_type {
            MarketType::CollateralIsQuote => ("long", "short"),
            MarketType::CollateralIsBase => ("short", "long"),
        };
        let (target_leverage_to_base_short, target_leverage_to_base_long) = match status.market_type
        {
            MarketType::CollateralIsQuote => {
                (target_leverage_to_notional, target_leverage_to_notional)
            }
            MarketType::CollateralIsBase => (
                target_leverage_to_notional + 1f64,
                target_leverage_to_notional - 1f64,
            ),
        };

        let funding_config = FundingConfig {
            funding_rate_max_annualized: status
                .config
                .funding_rate_max_annualized
                .to_string()
                .parse::<f64>()?,
            funding_rate_sensitivity: status
                .config
                .funding_rate_sensitivity
                .to_string()
                .parse::<f64>()?,
            delta_neutrality_fee_sensitivity: status
                .config
                .delta_neutrality_fee_sensitivity
                .to_string()
                .parse::<f64>()?,
            delta_neutrality_fee_cap: status
                .config
                .delta_neutrality_fee_cap
                .to_string()
                .parse::<f64>()?,
        };

        if has_long && has_short {
            res_str += "ERROR: There are both short and long positions open in the market.\n";
            if short_funding_notional > 0f64 {
                res_str += format!("Consider closing all {short_str} positions.\n").as_str();
            } else {
                res_str += format!("Consider closing all {long_str} positions.\n").as_str();
            }
            res_str += "Skipping market...\n";
            continue 'market;
        }

        if positions_count > 1 {
            res_str += "WARNING: More than one position open in the market, recommendations assume they are all combined into one.\n";
        }

        let short_and_decrease = |current_notional: f64| -> f64 {
            if current_notional > 0f64 {
                return 0f64;
            }
            bin_search(
                current_notional.max(-max_notional_size),
                0f64,
                |new_notional| -> bool {
                    OpenInterestAndFunding::new(
                        long_interest,
                        short_interest - current_notional.abs() + new_notional.abs(),
                        funding_config.clone(),
                    )
                    .short_funding
                        > cc_funding_short_close
                },
            )
        };

        let short_and_increase = |current_notional: f64| -> f64 {
            if -max_notional_size > current_notional {
                return -max_notional_size;
            }
            bin_search(
                -max_notional_size,
                current_notional.min(max_notional_size),
                |new_notional| -> bool {
                    OpenInterestAndFunding::new(
                        long_interest,
                        short_interest - current_notional.abs() + new_notional.abs(),
                        funding_config.clone(),
                    )
                    .short_funding
                        > cc_funding_short_open
                },
            )
        };

        let long_and_decrease = |current_notional: f64| -> f64 {
            if 0f64 > current_notional {
                return 0f64;
            }
            bin_search(
                0f64,
                current_notional.min(max_notional_size),
                |new_notional| -> bool {
                    OpenInterestAndFunding::new(
                        long_interest - current_notional.abs() + new_notional.abs(),
                        short_interest,
                        funding_config.clone(),
                    )
                    .long_funding
                        < cc_funding_long_close
                },
            )
        };

        let long_and_increase = |current_notional: f64| -> f64 {
            if current_notional > max_notional_size {
                return max_notional_size;
            }
            bin_search(
                current_notional.max(-max_notional_size),
                max_notional_size,
                |new_notional| -> bool {
                    OpenInterestAndFunding::new(
                        long_interest - current_notional.abs() + new_notional.abs(),
                        short_interest,
                        funding_config.clone(),
                    )
                    .long_funding
                        < cc_funding_long_open
                },
            )
        };

        let mut action_needed = false;
        let mut new_notional_size = total_notional_size;
        if total_notional_size < 0f64 {
            if short_funding_notional < cc_funding_short_open {
                new_notional_size = short_and_increase(total_notional_size);
                res_str += format!(
                    "UPDATE INCREASE {short_str} position size to have notional {} {notional}\n",
                    new_notional_size
                )
                .as_str();
                res_str += "    This can be achieved by:\n";
                res_str += format!(
                    "    1) Updating collateral impacting leverage to target {:.2}x leverage\n",
                    target_leverage_to_base_short
                )
                .as_str();
                res_str += format!("    2) Updating collateral impacting position size to target {} {collateral} collateral\n", price_notional * new_notional_size.abs() / target_leverage_to_notional).as_str();
                action_needed = true;
            } else if short_funding_notional > cc_funding_short_close {
                new_notional_size = short_and_decrease(total_notional_size);
                if new_notional_size == 0f64 {
                    res_str += format!("CLOSE {short_str} position\n").as_str();
                } else {
                    res_str += format!("UPDATE DECREASE {short_str} position size to have notional {} {notional}\n", new_notional_size).as_str();
                    res_str += "    This can be achieved by:\n";
                    res_str += format!(
                        "    1) Updating collateral impacting leverage to target {:.2}x leverage\n",
                        target_leverage_to_base_short
                    )
                    .as_str();
                    res_str += format!("    2) Updating collateral impacting position size to target {} {collateral} collateral\n", price_notional * new_notional_size.abs() / target_leverage_to_notional).as_str();
                }
                action_needed = true;
            }
        } else if total_notional_size > 0f64 {
            if long_funding_notional < cc_funding_long_open {
                new_notional_size = long_and_increase(total_notional_size);
                res_str += format!(
                    "UPDATE INCREASE {long_str} position size to have notional {} {notional}\n",
                    new_notional_size
                )
                .as_str();
                res_str += "    This can be achieved by:\n";
                res_str += format!(
                    "    1) Updating collateral impacting leverage to target {:.2}x leverage\n",
                    target_leverage_to_base_long
                )
                .as_str();
                res_str += format!("    2) Updating collateral impacting position size to target {} {collateral} collateral\n", price_notional * new_notional_size.abs() / target_leverage_to_notional).as_str();
                action_needed = true;
            } else if long_funding_notional > cc_funding_long_close {
                new_notional_size = long_and_decrease(total_notional_size);
                if new_notional_size == 0f64 {
                    res_str += format!("CLOSE {long_str} position\n").as_str();
                } else {
                    res_str += format!(
                        "UPDATE DECREASE {long_str} position size to have notional {} {notional}\n",
                        new_notional_size
                    )
                    .as_str();
                    res_str += "    This can be achieved by:\n";
                    res_str += format!(
                        "    1) Updating collateral impacting leverage to target {:.2}x leverage\n",
                        target_leverage_to_base_long
                    )
                    .as_str();
                    res_str += format!("    2) Updating collateral impacting position size to target {} {collateral} collateral\n", price_notional * new_notional_size.abs() / target_leverage_to_notional).as_str();
                }
                action_needed = true;
            }
        }

        if new_notional_size == 0f64 {
            let new_open_interest_and_funding = if total_notional_size > 0f64 {
                OpenInterestAndFunding::new(
                    long_interest - total_notional_size.abs(),
                    short_interest,
                    funding_config.clone(),
                )
            } else {
                OpenInterestAndFunding::new(
                    long_interest,
                    short_interest - total_notional_size.abs(),
                    funding_config.clone(),
                )
            };

            if new_open_interest_and_funding.long_funding < cc_funding_long_open {
                new_notional_size = bin_search(0f64, max_notional_size, |new_notional| -> bool {
                    OpenInterestAndFunding::new(
                        new_open_interest_and_funding.long_notional + new_notional.abs(),
                        new_open_interest_and_funding.short_notional,
                        funding_config.clone(),
                    )
                    .long_funding
                        < cc_funding_long_open
                });
                res_str += format!(
                    "OPEN A NEW {long_str} with notional {} {notional}\n",
                    new_notional_size
                )
                .as_str();
                res_str += "    This can be achieved by choosing:\n";
                res_str += format!(
                    "    1) {long_str} {:.2}x leverage\n",
                    target_leverage_to_base_long
                )
                .as_str();
                res_str += format!(
                    "    2) {} {collateral} collateral\n",
                    price_notional * new_notional_size.abs() / target_leverage_to_notional
                )
                .as_str();
                action_needed = true;
            } else if new_open_interest_and_funding.short_funding < cc_funding_short_open {
                new_notional_size = bin_search(-max_notional_size, 0f64, |new_notional| -> bool {
                    OpenInterestAndFunding::new(
                        new_open_interest_and_funding.long_notional,
                        new_open_interest_and_funding.short_notional + new_notional.abs(),
                        funding_config.clone(),
                    )
                    .short_funding
                        > cc_funding_short_open
                });
                res_str += format!(
                    "OPEN A NEW {short_str} with notional {} {notional}\n",
                    new_notional_size
                )
                .as_str();
                res_str += "    This can be achieved by choosing:\n";
                res_str += format!(
                    "    1) {short_str} {:.2}x leverage\n",
                    target_leverage_to_base_short
                )
                .as_str();
                res_str += format!(
                    "    2) {} {collateral} collateral\n",
                    price_notional * new_notional_size.abs() / target_leverage_to_notional
                )
                .as_str();
                action_needed = true;
            }
        }

        if !action_needed {
            res_str += "No action needed\n";
        }
    }

    Ok(res_str.into_response())
}
