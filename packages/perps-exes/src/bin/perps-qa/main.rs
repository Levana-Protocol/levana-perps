mod capping;
mod cli;

use crate::cli::Cmd;
use anyhow::Result;
use clap::Parser;
use cosmos::{Coin, Contract};
use msg::contracts::factory::entry::MarketsResp;
use msg::contracts::market::entry::StatusResp;
use msg::contracts::market::{entry::SlippageAssert, liquidity::LiquidityStats};
use perps_exes::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    main_inner().await
}

async fn main_inner() -> Result<()> {
    let Cmd { opt, subcommand }: Cmd = Cmd::parse();
    opt.init_logger();
    let client = reqwest::Client::new();
    let ConnectionInfo {
        network,
        factory_address,
        faucet_address,
    } = ConnectionInfo::load(
        &client,
        &opt.contract_family,
        opt.network,
        opt.factory_contract_address,
        opt.faucet_contract_address,
    )
    .await?;

    log::debug!("Factory address: {}", factory_address);

    let mut builder = network.builder().await?;
    if let Some(grpc) = opt.cosmos_grpc {
        builder.grpc_url = grpc;
    }
    let cosmos = builder.build().await?;

    if let cli::Subcommand::TransferDaoFees {} = subcommand {
        let builder = network.builder().await?;
        let cosmos = builder.build().await?;
        let factory_contract = Contract::new(cosmos.clone(), factory_address);

        let resp: MarketsResp = factory_contract
            .query(&msg::contracts::factory::entry::QueryMsg::Markets {
                start_after: None,
                limit: None,
            })
            .await?;
        for market_id in resp.markets {
            log::debug!("transferring dao fees for market id {market_id}");
            let app = PerpApp::new(
                opt.wallet.clone(),
                factory_address,
                Some(faucet_address),
                market_id,
                network,
            )
            .await?;
            log::debug!("wallet address {}", app.wallet_address);

            let resp = app.transfer_dao_fees().await?;
            log::info!("success: {}", resp.txhash);
        }
        Ok(())
    } else {
        let perp_contract = PerpApp::new(
            opt.wallet,
            factory_address,
            Some(faucet_address),
            opt.market_id,
            network,
        )
        .await?;

        match subcommand {
            cli::Subcommand::PrintBalances {} => {
                println!("Wallet address: {}", perp_contract.wallet_address);
                let balances = cosmos.all_balances(perp_contract.wallet_address).await?;
                for Coin { denom, amount } in &balances {
                    println!("{amount}{denom}");
                }
                if balances.is_empty() {
                    println!("0");
                }
                let balance = perp_contract.cw20_balance().await?;
                println!("Cw20 Balance: {}", balance);
            }
            cli::Subcommand::TotalPosition {} => {
                let count = perp_contract.market.total_positions().await?;
                println!("Total Open positions in Contract: {count}");
            }
            cli::Subcommand::AllOpenPositions {} => {
                let positions = perp_contract.all_open_positions().await?;
                let ids: Vec<_> = positions.ids.iter().map(|item| item.u64()).collect();
                println!(
                    "{} Open Positions in this wallet: {:?}",
                    positions.ids.len(),
                    ids
                );
                let (long_positions, short_positions): (Vec<_>, Vec<_>) = positions
                    .info
                    .iter()
                    .partition(|item| item.direction_to_base == DirectionToBase::Long);

                println!(
                    "{} Total long positions: {:?}",
                    long_positions.len(),
                    long_positions
                        .iter()
                        .map(|item| item.id.u64())
                        .collect::<Vec<_>>()
                );
                println!(
                    "{} Total short positions: {:?}",
                    short_positions.len(),
                    short_positions
                        .iter()
                        .map(|item| item.id.u64())
                        .collect::<Vec<_>>()
                );
            }
            cli::Subcommand::OpenPosition {
                collateral,
                leverage,
                max_gains,
                current_price,
                max_slippage,
                short,
            } => {
                let direction = if short {
                    DirectionToBase::Short
                } else {
                    DirectionToBase::Long
                };
                // Convert from percentage to ratio representation.
                let max_gain = match max_gains {
                    MaxGainsInQuote::Finite(x) => MaxGainsInQuote::Finite(
                        NonZero::new(x.raw() / Decimal256::from_str("100").unwrap()).unwrap(),
                    ),
                    MaxGainsInQuote::PosInfinity => MaxGainsInQuote::PosInfinity,
                };
                log::debug!("Collateral: {collateral}");
                log::debug!("Max gains: {:?}", max_gain);
                log::debug!("Leverage: {:?}", leverage);
                log::debug!("Direction: {:?}", direction);

                let slippage_assert = match (current_price, max_slippage) {
                    (None, None) => None,
                    (Some(current_price), Some(max_slippage)) => {
                        let tolerance = max_slippage / 100;
                        log::debug!("Current price: {}", current_price);
                        log::debug!("Tolerance: {}", tolerance);
                        Some(SlippageAssert {
                            price: current_price,
                            tolerance,
                        })
                    }
                    _ => anyhow::bail!(
                        "Must specify either both or neither of current price and max slippage"
                    ),
                };
                let tx = perp_contract
                    .open_position(
                        collateral,
                        direction,
                        leverage,
                        max_gain,
                        slippage_assert,
                        None,
                        None,
                    )
                    .await?;
                println!("Transaction hash: {}", tx.txhash);
                log::debug!("Raw log: {}", tx.raw_log);
            }
            cli::Subcommand::FetchPrice {} => {
                let price = perp_contract.market.current_price().await?;
                println!(
                    "Latest price of base asset (in quote): {}",
                    price.price_base
                );
                println!(
                    "Latest price of collater asset (in USD): {}",
                    price.price_usd
                );
            }
            cli::Subcommand::SetPrice { price, price_usd } => {
                let tx = perp_contract.set_price(price, price_usd).await?;
                println!("Transaction hash: {}", tx.txhash);
                log::debug!("Raw log: {}", tx.raw_log);
            }
            cli::Subcommand::ClosePosition { position_id } => {
                let tx = perp_contract.close_position(position_id).await?;
                println!("Transaction hash: {}", tx.txhash);
                log::debug!("Raw log: {}", tx.raw_log);
            }
            cli::Subcommand::Crank {} => {
                perp_contract.crank(None).await?;
            }
            cli::Subcommand::AllClosePositions {} => {
                let closed_positions = perp_contract.get_closed_positions().await?;
                let positions: Vec<_> = closed_positions.iter().map(|item| item.id.u64()).collect();
                println!("{} Closed positions: {:?}", positions.len(), positions);
            }
            cli::Subcommand::PositionDetail { position_id } => {
                let position = perp_contract.market.position_detail(position_id).await?;
                let liquidation_price = position
                    .liquidation_price_base
                    .map_or("No price found".to_owned(), |item| item.to_string());
                let take_profit_price = position
                    .take_profit_price_base
                    .map_or("No price found".to_owned(), |item| item.to_string());
                println!("Collateral: {}", position.deposit_collateral);
                println!("Active Collateral: {}", position.active_collateral);
                println!(
                    "Direction : {}",
                    match position.direction_to_base {
                        DirectionToBase::Long => "long",
                        DirectionToBase::Short => "short",
                    }
                );
                println!("Leverage: {}", position.leverage);
                println!("Max gains: {}", position.max_gains_in_quote);
                println!("Liquidation Price: {}", liquidation_price);
                println!("Profit price: {}", take_profit_price);
            }
            cli::Subcommand::TapFaucet {} => {
                let tx = perp_contract.tap_faucet().await?;
                println!("Transaction hash: {}", tx.txhash);
                log::debug!("Raw log: {}", tx.raw_log);
            }
            cli::Subcommand::UpdateMaxGains {
                position_id,
                max_gains,
            } => {
                let tx = perp_contract
                    .update_max_gains(position_id, max_gains)
                    .await?;
                println!("Transaction hash: {}", tx.txhash);
                log::debug!("Raw log: {}", tx.raw_log);
            }
            cli::Subcommand::UpdateCollateral {
                position_id,
                collateral,
                current_price,
                max_slippage,
                impact,
            } => {
                log::debug!("Collateral: {}", collateral);
                let slippage_assert = slippage_assert(current_price, max_slippage);
                let tx = perp_contract
                    .update_collateral(position_id, collateral, impact, slippage_assert)
                    .await?;
                println!("Transaction hash: {}", tx.txhash);
                log::debug!("Raw log: {}", tx.raw_log);
            }
            cli::Subcommand::UpdateLeverage {
                position_id,
                leverage,
                current_price,
                max_slippage,
            } => {
                let slippage_assert = slippage_assert(current_price, max_slippage);
                let tx = perp_contract
                    .update_leverage(position_id, leverage, slippage_assert)
                    .await?;
                println!("Transaction hash: {}", tx.txhash);
                log::debug!("Raw log: {}", tx.raw_log);
            }
            cli::Subcommand::Stats {} => {
                let StatusResp {
                    long_notional,
                    short_notional,
                    long_usd,
                    short_usd,
                    instant_delta_neutrality_fee_value,
                    liquidity:
                        LiquidityStats {
                            locked,
                            unlocked,
                            total_lp,
                            total_xlp,
                        },
                    ..
                } = perp_contract.market.status().await?;

                println!("Locked collateral: {locked}");
                println!("Unlocked collateral: {unlocked}");
                println!("Total LP tokens: {total_lp}");
                println!("Total xLP tokens: {total_xlp}");
                println!("Open long interest (in notional): {long_notional}");
                println!("Open short interest (in notional): {short_notional}");
                println!(
                    "Total interest (in notional): {}",
                    long_notional + short_notional
                );
                println!(
                    "Net interest (in notional): {}",
                    long_notional.into_signed() - short_notional.into_signed()
                );
                println!("Open long interest (in USD): {long_usd}");
                println!("Open short interest (in USD): {short_usd}");
                println!("Total interest (in USD): {}", long_usd + short_usd);
                println!(
                    "Net interest (in USD): {}",
                    long_usd.into_signed() - short_usd.into_signed()
                );
                println!(
                    "Instant delta neutrality: {}",
                    instant_delta_neutrality_fee_value
                );
            }
            cli::Subcommand::GetConfig {} => {
                let config = perp_contract.market.status().await?.config;
                println!("{config:?}");
            }
            cli::Subcommand::DepositLiquidity { fund } => {
                let tx = perp_contract.deposit_liquidity(fund).await?;
                println!("Transaction hash: {}", tx.txhash);
                log::debug!("Raw log: {}", tx.raw_log);
            }
            cli::Subcommand::CappingReport { inner } => inner.go(perp_contract).await?,
            cli::Subcommand::TransferDaoFees {} => {
                unreachable!("This is a placeholder for the real transfer dao fees command which was executed above")
            }
        }
        Ok(())
    }
}

fn slippage_assert(
    current_price: Option<PriceBaseInQuote>,
    max_slippage: Option<Number>,
) -> Option<SlippageAssert> {
    match (current_price, max_slippage) {
        (Some(price), Some(tolerance)) => Some(SlippageAssert { price, tolerance }),
        (None, None) => None,
        // The below two cases are not possible because of how clap operates
        (None, Some(_)) => None,
        (Some(_), None) => None,
    }
}
