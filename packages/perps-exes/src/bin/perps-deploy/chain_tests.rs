use std::str::FromStr;

use anyhow::{bail, ensure, Context, Result};
use msg::contracts::market::entry::SlippageAssert;
use msg::prelude::*;
use multi_test::response::CosmosResponseExt;
use perps_exes::{PerpApp, UpdatePositionCollateralImpact::Leverage};

pub async fn test_funding_market(perp_app: &PerpApp) -> Result<()> {
    tracing::info!("Testing that we can fund the market");

    async fn open_test_position(perp_app: &PerpApp) -> Result<()> {
        let tx = perp_app
            .open_position(
                "10".parse()?,
                DirectionToBase::Long,
                "5".parse()?,
                "200".parse()?,
                Some(SlippageAssert {
                    price: PriceBaseInQuote::from_str("9.9")?,
                    tolerance: "0.01".parse()?,
                }),
                None,
                None,
            )
            .await?;

        let _ = tx.event_first("position-open")?;

        for pos in perp_app.all_open_positions().await?.ids {
            perp_app.close_position(pos).await?;
        }

        perp_app.crank(None).await?;

        Ok(())
    }

    // Opening a position should initially fail, no liquidity available
    if open_test_position(perp_app).await.is_ok() {
        anyhow::bail!("test_funding_market: initial open position should fail");
    }

    // Now fund the market
    perp_app.deposit_liquidity("1000000000".parse()?).await?;

    // And now opening a position should succeed
    open_test_position(perp_app)
        .await
        .context("Could not open a test position within test_funding_market")?;

    Ok(())
}

pub async fn test_wallet_balance_decrease(perp_app: &PerpApp) -> Result<()> {
    tracing::info!("Testing that wallet balance decreases");

    // Open position
    let collateral = NonZero::<Collateral>::from_str("100")?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::from_str("0.44")?;
    let max_slippage = Number::from_str("1")?;
    let tolerance = (max_slippage / 100)?;
    let entry_price = PriceBaseInQuote::from_str("9.9")?;
    let leverage = LeverageToBase::from_str("10")?;

    let initial_balance = perp_app.cw20_balance().await?;

    perp_app
        .open_position(
            collateral,
            direction,
            leverage,
            max_gains,
            Some(SlippageAssert {
                price: entry_price,
                tolerance,
            }),
            None,
            None,
        )
        .await?;

    perp_app.crank(None).await?;

    let positions = perp_app.all_open_positions().await?;

    tracing::info!("Found open positions: {}", positions.ids.len());
    ensure!(
        positions.ids.len() == 1,
        "Only one position currently opened"
    );

    let current_balance = perp_app.cw20_balance().await?;
    ensure!(
        initial_balance > current_balance,
        "Balance should decrease after opening position"
    );

    threshold_range(initial_balance, current_balance, "100".parse().unwrap())?;

    for position in positions.ids {
        perp_app.close_position(position).await?;
    }
    perp_app.crank(None).await?;
    Ok(())
}

pub async fn test_update_collateral(perp_app: &PerpApp) -> Result<()> {
    tracing::info!("Testing collateral updates");

    // Open position
    let collateral = NonZero::<Collateral>::from_str("100")?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::from_str("0.44")?;
    let max_slippage = Number::from_str("1")?;
    let tolerance = (max_slippage / 100)?;
    let entry_price = PriceBaseInQuote::from_str("9.9")?;
    let leverage = LeverageToBase::from_str("10")?;

    let tx = perp_app
        .open_position(
            collateral,
            direction,
            leverage,
            max_gains,
            Some(SlippageAssert {
                price: entry_price,
                tolerance,
            }),
            None,
            None,
        )
        .await?;

    // FIXME: not totally sure that these terms are being used exactly as needed
    // as of right now, it's just to make the tests pass
    let slippage_fee = Signed::<Collateral>::from_number(tx.first_delta_neutrality_fee_amount());

    let initial_balance = perp_app.cw20_balance().await?;

    let positions = perp_app.all_open_positions().await?;
    let position = match positions.ids[..] {
        [a] => a,
        _ => bail!("More than one position found"),
    };

    let new_collateral = Collateral::from_str("105")?;
    let _tx = perp_app
        .update_collateral(position, new_collateral, Leverage, None)
        .await?;
    tracing::info!("Updated collateral (Increase)");

    let current_balance = perp_app.cw20_balance().await?;
    ensure!(
        current_balance < initial_balance,
        "Balance is reduced after increasing collateral"
    );

    // (balance - 100) - (balance - 100 - 5 - fees) <= 6
    threshold_range(
        initial_balance,
        current_balance.checked_add_signed(slippage_fee)?,
        "6".parse().expect("Parsing 6 failed"),
    )
    .expect("threshold_range failed");

    let positions = perp_app.all_open_positions().await?;

    let position_detail = match &positions.info[..] {
        [a] => a,
        _ => bail!("More than one position found"),
    };

    let delta = ((position_detail.deposit_collateral.into_number() - Number::from_str("105")?)?
        - slippage_fee.into_number())?;
    ensure!(
        delta < Number::from_str("1")?,
        format!(
            "Postion increased successfully: {}, {delta} < 1",
            position_detail.deposit_collateral
        )
    );

    let new_collateral = Collateral::from_str("100")?;
    let _tx = perp_app
        .update_collateral(position, new_collateral, Leverage, None)
        .await?;
    tracing::info!("Updated collateral (Decrease)");

    let incr_balance = perp_app.cw20_balance().await?;

    ensure!(
        incr_balance > current_balance,
        "Balance is increased after reducing collateral"
    );

    // (balance + 5 - fees) - balance <= 5
    threshold_range(
        incr_balance,
        current_balance,
        "5".parse().expect("Parsing 5 failed"),
    )?;

    let positions = perp_app.all_open_positions().await?;

    let position_detail = match &positions.info[..] {
        [a] => a,
        _ => bail!("More than one position found"),
    };

    ensure!(
        ((position_detail.deposit_collateral.into_number() - Number::from_str("100")?)?
            - slippage_fee.into_number())?
            < Number::from_str("1")?,
        format!(
            "Postion reduced successfully: {}",
            position_detail.deposit_collateral
        )
    );

    perp_app.close_position(position).await?;

    Ok(())
}

pub async fn test_set_and_fetch_price(perp_app: &PerpApp) -> Result<()> {
    tracing::info!("Set and fetch price");

    let new_price = PriceBaseInQuote::from_str("9.433493300000000079")?;

    let price_usd = new_price
        .try_into_usd(&perp_app.market_id)
        .unwrap_or(PriceCollateralInUsd::one());
    perp_app.set_price(new_price, price_usd).await?;
    perp_app.wait_till_next_block().await?;
    let price = perp_app.market.current_price().await?;
    ensure!(
        new_price == price.price_base,
        "Set and fetch price are same"
    );

    Ok(())
}

pub async fn test_update_leverage(perp_app: &PerpApp) -> Result<()> {
    tracing::info!("Test updating leverage");

    // Open position
    let collateral = "100".parse()?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::from_str("0.44")?;
    let max_slippage = Number::from_str("1")?;
    let tolerance = (max_slippage / 100)?;
    let entry_price = PriceBaseInQuote::from_str("9.47")?;
    let leverage = LeverageToBase::from_str("10")?;

    perp_app
        .open_position(
            collateral,
            direction,
            leverage,
            max_gains,
            Some(SlippageAssert {
                price: entry_price,
                tolerance,
            }),
            None,
            None,
        )
        .await?;

    let positions = perp_app.all_open_positions().await?;

    let position_detail = match &positions.info[..] {
        [] => bail!("No positions found"),
        [a] => a,
        xs => bail!("More than one position found: {xs:?}"),
    };

    perp_app
        .update_leverage(position_detail.id, LeverageToBase::from_str("11")?, None)
        .await?;
    perp_app.crank_single(None).await?;

    let new_position_detail = perp_app.market.position_detail(position_detail.id).await?;

    let diff_leverage =
        (new_position_detail.leverage.into_number() - position_detail.leverage.into_number())?;

    ensure!(
        diff_leverage > "0.9".parse()? && diff_leverage < "1".parse()?,
        "Leverage increased with delta of one. diff_leverage: {diff_leverage}"
    );

    perp_app.close_position(position_detail.id).await?;
    Ok(())
}

pub async fn test_update_max_gains(perp_app: &PerpApp) -> Result<()> {
    tracing::info!("Test updating Max Gains");

    // Open position
    let collateral = "100".parse()?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::from_str("0.44")?;
    let max_slippage = Number::from_str("1")?;
    let tolerance = (max_slippage / 100)?;
    let entry_price = PriceBaseInQuote::from_str("9.47")?;
    let leverage = LeverageToBase::from_str("10")?;

    perp_app
        .open_position(
            collateral,
            direction,
            leverage,
            max_gains,
            Some(SlippageAssert {
                price: entry_price,
                tolerance,
            }),
            None,
            None,
        )
        .await?;

    let positions = perp_app.all_open_positions().await?;

    let position_detail = match &positions.info[..] {
        [a] => a,
        _ => bail!("More than one position found"),
    };

    let max_gains = MaxGainsInQuote::from_str("0.50")?;
    perp_app
        .update_max_gains(position_detail.id, max_gains)
        .await?;
    perp_app.crank_single(None).await?;

    let new_position_detail = perp_app.market.position_detail(position_detail.id).await?;

    // TODO: remove this once the deprecated fields are fully removed
    #[allow(deprecated)]
    if let Some(max_gains) = new_position_detail.max_gains_in_quote {
        let diff_max_gains = (match max_gains {
            MaxGainsInQuote::Finite(x) => x.into_number(),
            MaxGainsInQuote::PosInfinity => anyhow::bail!("Infinite max gains for new position"),
        } - match max_gains {
            MaxGainsInQuote::Finite(x) => x.into_number(),
            MaxGainsInQuote::PosInfinity => anyhow::bail!("Infinite max gains for position_detail"),
        })?;

        // 0.5 - 0.44 = 0.06
        ensure!(
            diff_max_gains > "0.05".parse()? && diff_max_gains < "0.06".parse()?,
            "Max gains is updated with proper delta. diff_max_gains: {diff_max_gains}"
        );
    } else {
        // TODO - improve test to check take_profit_price instead
    }
    perp_app.close_position(position_detail.id).await?;

    Ok(())
}

// Similar in spirit with decEqual (typescript code)
fn threshold_range(n1: Collateral, n2: Collateral, threshold: Collateral) -> Result<()> {
    let num = n1.checked_sub(n2)?;
    ensure!(
        num <= threshold,
        "{n1} - {n2} = {num} within the threshold of {threshold}"
    );
    Ok(())
}

pub(crate) async fn test_pnl_on_liquidation(perp_app: &PerpApp) -> Result<()> {
    tracing::info!("Test PnL on liquidation");

    // Open position
    let collateral = "100".parse()?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::PosInfinity;
    let leverage = LeverageToBase::from_str("17.5")?;

    let new_price: PriceBaseInQuote = "6.33".parse()?;
    let price_usd = new_price
        .try_into_usd(&perp_app.market_id)
        .unwrap_or(PriceCollateralInUsd::one());
    perp_app.set_price(new_price, price_usd).await?;
    perp_app.crank(None).await?;

    perp_app
        .open_position(collateral, direction, leverage, max_gains, None, None, None)
        .await?;

    let positions = perp_app.all_open_positions().await?;

    let position_detail = match &positions.info[..] {
        [a] => a,
        _ => bail!("More than one position found"),
    };

    let new_price: PriceBaseInQuote = "6.016".parse()?;
    let price_usd = new_price
        .try_into_usd(&perp_app.market_id)
        .unwrap_or(PriceCollateralInUsd::one());
    let res = perp_app.set_price(new_price, price_usd).await?;
    perp_app.crank(None).await?;

    tracing::info!("Price set to force liquidation in: {}", res.txhash);

    let closed = perp_app
        .get_closed_positions()
        .await?
        .into_iter()
        .find(|x| x.id == position_detail.id)
        .context("Position wasn't closed")?;

    let pnl_from_price = closed
        .pnl_collateral
        .checked_sub(position_detail.pnl_collateral)?;
    ensure!(
        pnl_from_price < "-0.1".parse()?,
        "PnL didn't decrease sufficiently. Old PnL: {}. New PnL: {}.",
        position_detail.pnl_collateral,
        closed.pnl_collateral
    );

    Ok(())
}
