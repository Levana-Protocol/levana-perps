#![allow(dead_code, unused_variables, unreachable_code)] // FIXME see https://phobosfinance.atlassian.net/browse/PERP-548
use std::sync::Arc;

use anyhow::{Context, Result};
use cosmos::{
    proto::cosmwasm::wasm::v1::MsgExecuteContract, Coin, Cosmos, HasAddress, TxBuilder, Wallet,
};
use cosmwasm_std::{from_binary, Addr, WasmMsg};
use msg::contracts::market::entry::StatusResp;
use msg::prelude::*;
use msg::{
    contracts::{
        market,
        position_token::{self, entry::TokensResponse},
    },
    token::Token,
};
use parking_lot::RwLock;
use perps_exes::config::{DeploymentConfig, MarketConfig, NibbConfig};
use tokio::sync::Mutex;

use super::{
    factory::FactoryInfo,
    status_collector::{Status, StatusCategory, StatusCollector},
};
use crate::util::markets::{get_markets, Market};
use msg::contracts::cw20::entry::ExecuteMsg;
use msg::contracts::market::position::PositionId;

const POSITIONS_CHUNKS: u32 = 3;

struct Worker {
    config: Arc<DeploymentConfig>,
    nibb_config: Arc<NibbConfig>,
    wallet: Arc<Wallet>,
    cosmos: Cosmos,
    factory: Arc<RwLock<Arc<FactoryInfo>>>,
}

#[derive(Clone, Debug)]
struct NibbMarket {
    market: Market,
    config: MarketConfig,
}

/// Start the background thread to keep options pools up to date.
impl StatusCollector {
    pub(super) async fn start_perps_nibb(
        &self,
        cosmos: Cosmos,
        factory: Arc<RwLock<Arc<FactoryInfo>>>,
        config: Arc<DeploymentConfig>,
        nibb_config: Arc<NibbConfig>,

        wallet: Arc<Wallet>,
        gas_wallet: Arc<Mutex<Wallet>>,
    ) -> Result<()> {
        self.track_gas_funds(
            *wallet.address(),
            "nibb-bot",
            config.min_gas.nibb,
            gas_wallet,
        );

        let worker = Arc::new(Worker {
            config,
            wallet,
            cosmos,
            factory,
            nibb_config,
        });

        if true {
            self.add_status(
                StatusCategory::Nibb,
                "skipped",
                Status::success("Skipping nibb for now", None),
            )
        } else {
            // FIXME
            self.add_status_checks(StatusCategory::Nibb, UPDATE_DELAY_SECONDS, move || {
                worker.clone().balance_all()
            });
        }

        Ok(())
    }
}

const UPDATE_DELAY_SECONDS: u64 = 10;
const TOO_OLD_SECONDS: i64 = 2 * 60;

impl Worker {
    async fn balance_all(self: Arc<Self>) -> Vec<(String, Status)> {
        let key = "get-markets".to_owned();
        let (mut res, markets) = match self.get_markets().await {
            Ok(markets) => {
                let status = (
                    key,
                    Status::success(
                        format!("Successfully loaded markets: {markets:?}"),
                        Some(TOO_OLD_SECONDS),
                    ),
                );
                (vec![status], markets)
            }
            Err(e) => return vec![(key, Status::error(format!("Unable to load markets: {e:?}")))],
        };

        let mut messages = vec![];

        for market in markets {
            let status_key = format!("check-crank-{}", market.market.market_id);
            let status = match self.check_crank(&market).await {
                Ok((status, needs_crank)) => {
                    if !needs_crank {
                        match self.balance(&market).await {
                            Ok(msgs) => {
                                for (status, msg) in msgs {
                                    messages.push(msg);
                                    res.push((
                                        market.market.market_id.to_string(),
                                        Status::success(
                                            format!("Balancing msgs generated: {status}"),
                                            None,
                                        ),
                                    ));
                                }
                            }
                            Err(e) => res.push((
                                format!("NIBB: {}", market.market.market_id),
                                Status::error(format!("Error while balancing: {e:?}")),
                            )),
                        }
                    }
                    status
                }
                Err(e) => Status::error(format!("Error while pre-balancing cranking: {e:?}")),
            };
            res.push((status_key, status));
        }

        let status = if messages.is_empty() {
            Status::success("No balancing required", Some(TOO_OLD_SECONDS))
        } else {
            let mut tx = TxBuilder::default();
            for msg in messages {
                tx = tx.add_message(msg);
            }
            match tx.sign_and_broadcast(&self.cosmos, &self.wallet).await {
                Ok(res) => Status::success(
                    format!(
                        "Successfully executed balancing messages, txhash == {}",
                        res.txhash
                    ),
                    Some(TOO_OLD_SECONDS),
                ),
                Err(e) => Status::error(format!("Error while executing balancing messages: {e:?}")),
            }
        };

        res.push(("execute".to_owned(), status));

        res
    }

    async fn get_markets(&self) -> Result<Vec<NibbMarket>> {
        let factory = self.factory.read().factory;
        get_markets(&self.cosmos, factory)
            .await?
            .into_iter()
            .map(|market| {
                self.nibb_config
                    .markets
                    .get(&market.market_id)
                    .copied()
                    .with_context(|| {
                        format!(
                            "Balancer config for market id: {} missing",
                            market.market_id
                        )
                    })
                    .map(|config| NibbMarket { config, market })
            })
            .collect()
    }

    async fn get_all_positions(
        &self,
        market: &NibbMarket,
    ) -> Result<Vec<market::position::PositionQueryResponse>> {
        let mut start: Option<String> = None;
        let mut positions = vec![];

        loop {
            let token_ids: TokensResponse = market
                .market
                .position_token
                .query(position_token::entry::QueryMsg::Tokens {
                    owner: self.wallet.address().to_string().into(),
                    start_after: start,
                    limit: Some(POSITIONS_CHUNKS),
                })
                .await?;

            let position_ids = token_ids
                .tokens
                .iter()
                .map(|p| Ok(market::position::PositionId(p.parse()?)))
                .collect::<Result<Vec<PositionId>>>()?;

            let mut pos: Vec<market::position::PositionQueryResponse> = market
                .market
                .market
                .query(market::entry::QueryMsg::Positions {
                    position_ids,
                    skip_calc_pending_fees: false,
                })
                .await?;

            start = match pos.last() {
                None => break,
                Some(last) => Some(last.id.0.to_string()),
            };
            positions.append(&mut pos);
        }

        Ok(positions)
    }

    /// Returns `true` if cranking is necessary.
    async fn check_crank(&self, market: &NibbMarket) -> Result<(Status, bool)> {
        let market::entry::StatusResp {
            next_crank: work, ..
        } = market
            .market
            .market
            .query(market::entry::QueryMsg::Status {})
            .await?;

        let needs_crank = work.is_some();

        Ok((
            Status::success(
                if needs_crank {
                    "Needs to be cranked"
                } else {
                    "No cranking necessary"
                },
                Some(TOO_OLD_SECONDS),
            ),
            needs_crank,
        ))
    }

    async fn balance(&self, market: &NibbMarket) -> Result<Vec<(String, MsgExecuteContract)>> {
        let mut messages = vec![];

        let market_contract = &market.market.market;

        let status: StatusResp = market_contract
            .query(market::entry::QueryMsg::Status {})
            .await?;

        let funding_rate_sensitivity = status.config.funding_rate_sensitivity;
        let wallet_source = status.collateral;
        let open_long_interest = status.long_notional;
        let open_short_interest = status.short_notional;
        let long_funding = status.long_funding;
        let short_funding = status.short_funding;

        let funding = if long_funding > Number::ZERO {
            long_funding
        } else {
            -short_funding
        };

        let price_point: PricePoint = market_contract
            .query(market::entry::QueryMsg::SpotPrice { timestamp: None })
            .await?;

        let min_range = market.config.target_mid_funding_rates - market.config.funding_rates_range;
        let max_range = market.config.target_mid_funding_rates + market.config.funding_rates_range;

        let target_funding_rates = if funding < min_range {
            Some(min_range)
        } else if funding > max_range {
            Some(max_range)
        } else {
            None
        };

        if let Some(target_funding_rates) = target_funding_rates {
            let positions = self.get_all_positions(market).await?;

            let notional_delta_numerator = target_funding_rates * open_long_interest.into_number()
                + target_funding_rates * open_short_interest.into_number()
                - funding_rate_sensitivity.into_signed() * open_long_interest.into_number()
                + open_short_interest.into_number() * funding_rate_sensitivity.into_signed();

            let (overweight_direction, delta_size) = if funding > target_funding_rates {
                (
                    DirectionToBase::Short,
                    notional_delta_numerator
                        / (-funding_rate_sensitivity.into_signed() + target_funding_rates),
                )
            } else {
                (
                    DirectionToBase::Long,
                    notional_delta_numerator
                        / (funding_rate_sensitivity.into_signed() + target_funding_rates),
                )
            };

            // Try find a position to close
            for position in positions {
                if position.direction_to_base == overweight_direction
                    && position.notional_size.abs().into_number() <= delta_size.abs()
                {
                    messages.push((
                        format!(
                            "Closing {:?} with notional_size: {:?}",
                            position.direction_to_base, position.notional_size
                        ),
                        MsgExecuteContract {
                            sender: self.wallet.to_string(),
                            contract: market.market.market.get_address_string(),
                            msg: serde_json::to_vec(&market::entry::ExecuteMsg::ClosePosition {
                                id: position.id,
                                slippage_assert: None,
                            })?,
                            funds: vec![],
                        },
                    ));

                    return Ok(messages);
                }
            }

            // Open for remainings
            if delta_size.abs() >= market.config.delta_size_threshold {
                let (collateral, exposure_leverage_to_collateral, max_gains_in_notional) =
                    if overweight_direction == DirectionToBase::Long {
                        // Open short
                        let notional_delta_denominator =
                            -funding_rate_sensitivity.into_signed() - target_funding_rates;
                        let notional_delta = Signed::<Notional>::from_number(
                            notional_delta_numerator / notional_delta_denominator,
                        );
                        let collateral =
                            notional_delta.map(|x| price_point.notional_to_collateral(x));
                        let exposure_leverage_to_collateral: Number = "2".parse()?;
                        let max_gains_in_notional = MaxGainsInQuote::PosInfinity;
                        (
                            collateral,
                            exposure_leverage_to_collateral,
                            max_gains_in_notional,
                        )
                    } else {
                        // Open long
                        let notional_delta_denominator =
                            funding_rate_sensitivity.into_signed() - target_funding_rates;
                        let notional_delta = notional_delta_numerator / notional_delta_denominator;
                        let collateral = Signed::<Notional>::from_number(
                            Number::from_ratio_u256(1u32, 2u32) * notional_delta.into_number(),
                        )
                        .map(|x| price_point.notional_to_collateral(x));
                        let exposure_leverage_to_collateral: Number = Number::NEG_ONE;
                        let max_gains_in_notional = MaxGainsInQuote::Finite("0.5".parse().unwrap());
                        (
                            collateral,
                            exposure_leverage_to_collateral,
                            max_gains_in_notional,
                        )
                    };

                let collateral =
                    NonZero::try_from_signed(collateral).context("Collateral must be GT zero")?;

                let (contract_addr, msg, funds) = if let WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds,
                } = wallet_source.into_market_execute_msg(
                    &Addr::unchecked(market.market.market.get_address_string()),
                    collateral.raw(),
                    market::entry::ExecuteMsg::OpenPosition {
                        leverage: "invalid value".parse()?,
                        max_gains: "invalid value".parse()?,
                        direction: DirectionToBase::Long,
                        slippage_assert: None,
                        stop_loss_override: None,
                        take_profit_override: None,
                    },
                )? {
                    (contract_addr, msg, funds)
                } else {
                    anyhow::bail!(
                        "Invalid msg prepared for {:?} with collateral {:?} on market: {:?}",
                        wallet_source,
                        collateral,
                        market
                    );
                };

                let (msg, funds) = match wallet_source {
                    Token::Cw20 { .. } => {
                        let msg: ExecuteMsg = from_binary(&msg)?;
                        (serde_json::to_vec(&msg)?, vec![])
                    }
                    Token::Native { .. } => {
                        let msg: market::entry::ExecuteMsg = from_binary(&msg)?;
                        (
                            serde_json::to_vec(&msg)?,
                            funds
                                .iter()
                                .map(|c| Coin {
                                    denom: c.denom.clone(),
                                    amount: c.amount.to_string(),
                                })
                                .collect(),
                        )
                    }
                };

                messages.push((
                    format!(
                        "Opening position: col: {:?}, exp_lvg: {:?}, max_gains: {:?}",
                        collateral, exposure_leverage_to_collateral, max_gains_in_notional
                    ),
                    MsgExecuteContract {
                        sender: self.wallet.get_address_string(),
                        contract: contract_addr,
                        msg,
                        funds,
                    },
                ));
            }
        }

        Ok(messages)
    }
}
