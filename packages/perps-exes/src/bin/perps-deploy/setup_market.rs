use anyhow::Result;
use cosmos::HasAddress;
use msg::contracts::tracker::entry::ContractResp;
use msg::prelude::*;
use perps_exes::PerpApp;

use crate::cli::Opt;
use crate::store_code::FACTORY;

#[derive(clap::Parser)]
pub(crate) struct SetupMarketOpt {
    /// Family name for these contracts
    #[clap(long, env = "PERPS_FAMILY")]
    family: String,
    /// Which market to set up
    #[clap(long)]
    market: MarketId,
    /// How much liquidity we want in the market
    #[clap(long, default_value = "10000000")]
    target_liquidity: Collateral,
    /// Deposit collateral for each newly opened position
    #[clap(long, default_value = "2000")]
    deposit: NonZero<Collateral>,
}

pub(crate) async fn go(
    opt: Opt,
    SetupMarketOpt {
        family,
        market,
        target_liquidity,
        deposit,
    }: SetupMarketOpt,
) -> Result<()> {
    let app = opt.load_app(&family).await?;

    let factory = match app
        .tracker
        .get_contract_by_family(FACTORY, &family, None)
        .await?
    {
        ContractResp::NotFound {} => anyhow::bail!("Factory contract not found"),
        ContractResp::Found { address, .. } => address.parse()?,
    };

    let app = PerpApp::new(
        opt.wallet.context("No wallet provided")?,
        factory,
        Some(app.faucet.get_address()),
        market,
        app.basic.network,
    )
    .await?;

    let mut iter_count = 0;

    loop {
        iter_count += 1;
        log::info!("Setup iteration #{iter_count}");

        let status = app.market.status().await?;

        let total = status.liquidity.locked + status.liquidity.unlocked;
        log::info!("Total liquidity: {total}. Target liquidity: {target_liquidity}");

        if let Some(delta) = target_liquidity
            .checked_sub(total)
            .ok()
            .and_then(NonZero::new)
        {
            // Rounding errors
            let delta = delta.checked_add(Collateral::one())?;
            log::info!("Need to deposit additional {delta} collateral");
            let res = app.deposit_liquidity(delta).await?;
            log::info!("Deposited in {}", res.txhash);
            continue;
        }

        let util = status.liquidity.locked.into_decimal256() / total.into_decimal256();
        let target = status.config.target_utilization.raw();
        log::info!("Utilization ratio: {util}. Target: {target}.");
        if util >= target {
            log::info!("Sufficient utilization, all done!");
            break;
        }

        // Open an unpopular position
        let direction = if status.long_notional > status.short_notional {
            DirectionToBase::Short
        } else {
            DirectionToBase::Long
        };
        let res = app
            .open_position(
                deposit,
                direction,
                "12".parse().unwrap(),
                match direction {
                    DirectionToBase::Long => MaxGainsInQuote::PosInfinity,
                    DirectionToBase::Short => "5".parse().unwrap(),
                },
                None,
                None,
                None,
            )
            .await?;
        log::info!(
            "Opened new {} position at {}",
            direction.as_str(),
            res.txhash
        );
    }

    Ok(())
}
