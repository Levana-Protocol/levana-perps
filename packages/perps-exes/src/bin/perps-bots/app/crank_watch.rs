use anyhow::Result;
// use axum::async_trait;
// use chrono::{Duration, Utc};
// use cosmos::Wallet;
// use msg::contracts::market::crank::CrankWorkInfo;
// use perps_exes::prelude::MarketContract;
// use shared::storage::RawAddr;

// use crate::config::BotConfigByType;
// use crate::util::markets::Market;
// use crate::watcher::{WatchedTaskOutput, WatchedTaskPerMarket};

// use super::factory::FactoryInfo;
// use super::gas_check::GasCheckWallet;
use super::{crank_run::TriggerCrank, AppBuilder};

// #[derive(Clone)]
// struct Worker {
//     crank_wallet: Wallet,
// }

/// Start the background thread to turn the crank on the crank bots.
impl AppBuilder {
    pub(super) fn start_crank_watch(&mut self, _: TriggerCrank) -> Result<()> {
        todo!()
        // if let Some(crank_wallet) = self.app.config.crank_wallet.clone() {
        //     match &self.app.config.by_type {
        //         BotConfigByType::Testnet { inner } => {
        //             let inner = inner.clone();
        //             self.refill_gas(&inner, *crank_wallet.address(), GasCheckWallet::CrankRun)?
        //         }
        //         BotConfigByType::Mainnet { inner } => self.alert_on_low_gas(
        //             *crank_wallet.address(),
        //             GasCheckWallet::CrankRun,
        //             inner.min_gas_crank,
        //         )?,
        //     }

        //     let worker = Worker { crank_wallet };
        //     self.watch_periodic(crate::watcher::TaskLabel::Crank, worker)
        // } else {
        //     Ok(())
        // }
    }
}

// #[async_trait]
// impl WatchedTaskPerMarket for Worker {
//     async fn run_single_market(
//         &mut self,
//         app: &App,
//         _factory: &FactoryInfo,
//         market: &Market,
//     ) -> Result<WatchedTaskOutput> {
//         app.crank(&self.crank_wallet, &market.market).await
//     }
// }

// const CRANK_EXECS: &[u32] = &[30, 25, 20, 15, 10, 7, 6, 5, 4, 3, 2, 1];

// impl App {
//     async fn crank(
//         &self,
//         crank_wallet: &Wallet,
//         market: &MarketContract,
//     ) -> Result<WatchedTaskOutput> {
//         let work = match self.check_crank(market).await? {
//             None => {
//                 return Ok(WatchedTaskOutput {
//                     skip_delay: false,
//                     message: "No crank messages waiting".to_owned(),
//                 })
//             }
//             Some(work) => work,
//         };

//         for execs in CRANK_EXECS {
//             let crank_start = Utc::now();
//             let res = self
//                 .try_with_execs(
//                     crank_wallet,
//                     market,
//                     &work,
//                     Some(*execs),
//                     self.config.get_crank_rewards_wallet(),
//                 )
//                 .await;
//             let crank_time = Utc::now() - crank_start;
//             log::debug!("Crank for {execs} takes {crank_time}");
//             match res {
//                 Ok(x) => return Ok(x),
//                 Err(e) => log::warn!("Cranking with execs=={execs} failed: {e:?}"),
//             }
//         }

//         let crank_start = Utc::now();
//         let res = self
//             .try_with_execs(
//                 crank_wallet,
//                 market,
//                 &work,
//                 None,
//                 self.config.get_crank_rewards_wallet(),
//             )
//             .await;
//         let crank_time = Utc::now() - crank_start;
//         log::debug!("Crank for None takes {crank_time}");
//         res
//     }

//     async fn try_with_execs(
//         &self,
//         crank_wallet: &Wallet,
//         market: &MarketContract,
//         work: &CrankReason,
//         execs: Option<u32>,
//         rewards: Option<RawAddr>,
//     ) -> Result<WatchedTaskOutput> {
//         let txres = market.crank_single(crank_wallet, execs, rewards).await?;
//         Ok(WatchedTaskOutput {
//             skip_delay: true,
//             message: format!(
//                 "Successfully turned the crank for work item {work:?} in transaction {}",
//                 txres.txhash
//             ),
//         })
//     }

//     async fn check_crank(&self, market: &MarketContract) -> Result<Option<CrankReason>> {
//         let status = market.status().await?;
//         if let Some(work) = status.next_crank {
//             Ok(Some(CrankReason::WorkAvailable(work)))
//         } else {
//             match status.last_crank_completed {
//                 None => Ok(Some(CrankReason::NoPriorCrank)),
//                 Some(timestamp) => {
//                     let timestamp = timestamp.try_into_chrono_datetime()?;
//                     let now = Utc::now();
//                     let age = now.signed_duration_since(timestamp);
//                     if age.num_seconds() > MAX_CRANK_AGE {
//                         Ok(Some(CrankReason::OldLastCrank(age)))
//                     } else {
//                         Ok(None)
//                     }
//                 }
//             }
//         }
//     }
// }

// const MAX_CRANK_AGE: i64 = 60 * 10;

// #[derive(Debug)]
// enum CrankReason {
//     WorkAvailable(CrankWorkInfo),
//     OldLastCrank(Duration),
//     NoPriorCrank,
// }
