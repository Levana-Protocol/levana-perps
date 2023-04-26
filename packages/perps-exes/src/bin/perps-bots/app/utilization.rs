use std::sync::Arc;

use anyhow::Result;
use axum::async_trait;
use cosmos::{proto::cosmos::base::abci::v1beta1::TxResponse, Address, HasAddress, Wallet};
use msg::{contracts::market::entry::StatusResp, prelude::*};
use perps_exes::prelude::*;

use crate::watcher::{WatchedTaskOutput, WatchedTaskPerMarket};

use super::{factory::FactoryInfo, App, AppBuilder};

pub(super) struct Utilization {
    pub(super) app: Arc<App>,
    pub(super) wallet: Wallet,
}

impl AppBuilder {
    pub(super) fn launch_utilization(&mut self, wallet: Wallet) -> Result<()> {
        let util = Utilization {
            app: self.app.clone(),
            wallet,
        };
        self.watch_periodic(crate::watcher::TaskLabel::Utilization, util)
    }
}

#[async_trait]
impl WatchedTaskPerMarket for Utilization {
    async fn run_single_market(
        &self,
        _app: &App,
        factory: &FactoryInfo,
        _market: &MarketId,
        addr: Address,
    ) -> Result<WatchedTaskOutput> {
        single_market(self, addr, factory.faucet).await
    }
}

async fn single_market(
    worker: &Utilization,
    market_addr: Address,
    faucet: Address,
) -> Result<WatchedTaskOutput> {
    let market = MarketContract::new(worker.app.cosmos.make_contract(market_addr));
    let status = market.status().await?;
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

    if util > worker.app.config.utilization_config.max_util {
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
    } else if util < worker.app.config.utilization_config.min_util {
        log::info!("Low utilization ratio, opening positions.");

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
        if balance < "20000".parse().unwrap() {
            worker
                .app
                .config
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

        let res1 = open(worker, &status, &market, DirectionToBase::Long).await;
        let res2 = open(worker, &status, &market, DirectionToBase::Short).await;

        match (res1, res2) {
            (Err(e1), Err(e2)) => Err(anyhow::anyhow!(
                "Long and short both failed\n{e1:?}\n{e2:?}"
            )),
            (long, short) => Ok(WatchedTaskOutput {
                skip_delay: true,
                message: format!("Long: {:?}\nShort: {:?}", long.is_ok(), short.is_ok()),
            }),
        }
    } else {
        Ok(WatchedTaskOutput {
            skip_delay: false,
            message: "No work to do".to_owned(),
        })
    }
}

async fn open(
    worker: &Utilization,
    status: &StatusResp,
    market: &MarketContract,
    direction: DirectionToBase,
) -> Result<TxResponse> {
    market
        .open_position(
            &worker.wallet,
            status,
            "500".parse().unwrap(),
            direction,
            "8".parse().unwrap(),
            "3".parse().unwrap(),
            None,
            None,
            None,
        )
        .await
}
