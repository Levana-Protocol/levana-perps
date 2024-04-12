use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use msg::{contracts::market::entry::StatusResp, prelude::*};
use perps_exes::PerpApp;

#[derive(clap::Parser)]
pub(crate) struct Opt {
    #[clap(long, default_value = "capping-report.csv")]
    csv: PathBuf,
    /// How many blocks between each sample
    #[clap(long, default_value = "100")]
    block_step: u64,
    /// Total number of samples to take
    #[clap(long, default_value = "1008")]
    samples: u64,
}

impl Opt {
    pub(crate) async fn go(self, app: PerpApp) -> Result<()> {
        let Opt {
            csv,
            block_step,
            mut samples,
        } = self;
        let info = app.cosmos.get_latest_block_info().await?;
        let mut csv = csv::Writer::from_path(&csv)?;
        let mut next_height = info.height.try_into()?;

        while samples > 0 {
            samples -= 1;
            log::info!("Querying {next_height}");

            let status: StatusResp = app
                .market
                .status_at_height(next_height)
                .await
                .with_context(|| {
                    format!("Failed while querying status for block height {next_height}")
                })?;
            let timestamp = app
                .cosmos
                .get_block_info(next_height.try_into()?)
                .await?
                .timestamp;

            let net_notional =
                (status.long_notional.into_signed() - status.short_notional.into_signed())?;

            let largest_net_abs = Notional::from_decimal256(
                status.config.delta_neutrality_fee_sensitivity.raw()
                    * status.config.delta_neutrality_fee_cap.raw(),
            );

            csv.serialize(Record {
                height: next_height,
                timestamp,
                largest_long: calc_largest(net_notional, largest_net_abs),
                largest_short: calc_largest(-net_notional, largest_net_abs),
                unlocked_liquidity: status.liquidity.unlocked,
                net_notional,
            })?;
            csv.flush()?;

            match next_height.checked_sub(block_step) {
                None => break,
                Some(x) => next_height = x,
            }
        }

        Ok(())
    }
}

fn calc_largest(net_notional: Signed<Notional>, largest_net_abs: Notional) -> Notional {
    if net_notional > largest_net_abs.into_signed() {
        Notional::zero()
    } else {
        largest_net_abs
            .checked_add_signed(-net_notional)
            .expect("Cannot be negative!")
    }
}

#[derive(serde::Serialize)]
struct Record {
    height: u64,
    timestamp: DateTime<Utc>,
    largest_long: Notional,
    largest_short: Notional,
    unlocked_liquidity: Collateral,
    net_notional: Signed<Notional>,
}
