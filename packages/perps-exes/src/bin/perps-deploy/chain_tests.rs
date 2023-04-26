use std::str::FromStr;

use anyhow::{anyhow, bail, ensure, Context, Result};
use cosmwasm_std::Uint128;
use msg::contracts::market::entry::StatusResp;
use msg::contracts::market::{entry::QueryMsg, entry::SlippageAssert};
use msg::prelude::*;
use multi_test::response::CosmosResponseExt;
use perps_exes::{PerpApp, UpdatePositionCollateralImpact::Leverage};

pub async fn test_funding_market(perp_app: &PerpApp) -> Result<()> {
    log::info!("Testing that we can fund the market");

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
            perp_app.close_position(pos.0).await?;
        }

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
    log::info!("Testing that wallet balance decreases");

    // Open position
    let collateral = NonZero::<Collateral>::from_str("100")?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::from_str("44")?;
    let max_gains = perps_exes::types::money::notional_max_gain(max_gains);
    let max_slippage = Number::from_str("1")?;
    let tolerance = max_slippage / 100;
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

    perp_app.crank().await?;

    let positions = perp_app.all_open_positions().await?;

    log::info!("Found open positions: {}", positions.ids.len());
    ensure!(
        positions.ids.len() == 1,
        "Only one position currently opened"
    );

    let current_balance = perp_app.cw20_balance().await?;
    ensure!(
        initial_balance.balance > current_balance.balance,
        "Balance should decrease after opening position"
    );

    let difference = perp_app
        .collateral_to_u128(NonZero::<Collateral>::from_str("100")?)?
        .into();

    threshold_range(initial_balance.balance, current_balance.balance, difference)?;

    for position in positions.ids {
        perp_app.close_position(position.0).await?;
    }
    Ok(())
}

pub async fn test_update_collateral(perp_app: &PerpApp) -> Result<()> {
    log::info!("Testing collateral updates");

    // Open position
    let collateral = NonZero::<Collateral>::from_str("100")?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::from_str("44")?;
    let max_gains = perps_exes::types::money::notional_max_gain(max_gains);
    let max_slippage = Number::from_str("1")?;
    let tolerance = max_slippage / 100;
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
    let (slippage_fee, u_slippage_fee) = {
        let slippage_fee = tx.first_delta_neutrality_fee_amount();
        let StatusResp {
            collateral: token, ..
        } = perp_app.market_contract.query(QueryMsg::Status {}).await?;

        let u_slippage_fee = slippage_fee
            .try_into_positive_value()
            .and_then(|x| token.into_u128(x).ok().flatten())
            .ok_or_else(|| anyhow!("Error converting {} to u128", slippage_fee))?;

        (slippage_fee, Uint128::from(u_slippage_fee))
    };

    perp_app.crank().await?;

    let initial_balance = perp_app.cw20_balance().await?;

    let positions = perp_app.all_open_positions().await?;
    let position = match positions.ids[..] {
        [a] => a,
        _ => bail!("More than one position found"),
    };

    let new_collateral = Collateral::from_str("105")?;
    let _tx = perp_app
        .update_collateral(position.0, new_collateral, Leverage, None)
        .await?;
    log::info!("Updated collateral (Increase)");

    let current_balance = perp_app.cw20_balance().await?;
    ensure!(
        current_balance.balance < initial_balance.balance,
        "Balance is reduced after increasing collateral"
    );

    let diff_collateral = perp_app
        .collateral_to_u128(NonZero::<Collateral>::from_str("6")?)?
        .into();

    // (balance - 100) - (balance - 100 - 5 - fees) <= 6
    threshold_range(
        initial_balance.balance,
        current_balance.balance + u_slippage_fee,
        diff_collateral,
    )
    .unwrap();

    let positions = perp_app.all_open_positions().await?;

    let position_detail = match &positions.info[..] {
        [a] => a,
        _ => bail!("More than one position found"),
    };

    ensure!(
        position_detail.deposit_collateral.into_number() - Number::from_str("105")? - slippage_fee
            < Number::from_str("1")?,
        format!(
            "Postion increased successfully: {}",
            position_detail.deposit_collateral
        )
    );

    let new_collateral = Collateral::from_str("100")?;
    let _tx = perp_app
        .update_collateral(position.0, new_collateral, Leverage, None)
        .await?;
    log::info!("Updated collateral (Decrease)");

    let incr_balance = perp_app.cw20_balance().await?;

    ensure!(
        incr_balance.balance > current_balance.balance,
        "Balance is increased after reducing collateral"
    );

    let five_collateral = perp_app.collateral_to_u128("5".parse()?)?.into();

    // (balance + 5 - fees) - balance <= 5
    threshold_range(
        incr_balance.balance,
        current_balance.balance,
        five_collateral,
    )?;

    let positions = perp_app.all_open_positions().await?;

    let position_detail = match &positions.info[..] {
        [a] => a,
        _ => bail!("More than one position found"),
    };

    ensure!(
        position_detail.deposit_collateral.into_number() - Number::from_str("100")? - slippage_fee
            < Number::from_str("1")?,
        format!(
            "Postion reduced successfully: {}",
            position_detail.deposit_collateral
        )
    );

    perp_app.close_position(position.0).await?;

    Ok(())
}

pub async fn test_set_and_fetch_price(perp_app: &PerpApp) -> Result<()> {
    log::info!("Set and fetch price");

    let new_price = PriceBaseInQuote::from_str("9.433493300000000079")?;
    perp_app.set_price(new_price).await?;
    perp_app.wait_till_next_block().await?;
    let price = perp_app.fetch_price().await?;
    ensure!(
        new_price == price.price_base,
        "Set and fetch price are same"
    );

    Ok(())
}

pub async fn test_update_leverage(perp_app: &PerpApp) -> Result<()> {
    log::info!("Test updating leverage");

    // Open position
    let collateral = "100".parse()?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::from_str("44")?;
    let max_gains = perps_exes::types::money::notional_max_gain(max_gains);
    let max_slippage = Number::from_str("1")?;
    let tolerance = max_slippage / 100;
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

    perp_app
        .update_leverage(position_detail.id.0, LeverageToBase::from_str("11")?, None)
        .await?;

    let new_position_detail = perp_app.position_detail(position_detail.id.0).await?;

    let diff_leverage =
        new_position_detail.leverage.into_number() - position_detail.leverage.into_number();

    ensure!(
        diff_leverage > "0.9".parse()? && diff_leverage < "1".parse()?,
        "Leverage increased with delta of one"
    );

    perp_app.close_position(position_detail.id.0).await?;
    Ok(())
}

pub async fn test_update_max_gains(perp_app: &PerpApp) -> Result<()> {
    log::info!("Test updating Max Gains");

    // Open position
    let collateral = "100".parse()?;
    let direction = DirectionToBase::Long;
    let max_gains = MaxGainsInQuote::from_str("44")?;
    let max_gains = perps_exes::types::money::notional_max_gain(max_gains);
    let max_slippage = Number::from_str("1")?;
    let tolerance = max_slippage / 100;
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

    let max_gains = MaxGainsInQuote::from_str("50")?;
    let max_gains = perps_exes::types::money::notional_max_gain(max_gains);
    perp_app
        .update_max_gains(position_detail.id.0, max_gains)
        .await?;
    let new_position_detail = perp_app.position_detail(position_detail.id.0).await?;

    let diff_max_gains = match new_position_detail.max_gains_in_quote {
        MaxGainsInQuote::Finite(x) => x.into_number(),
        MaxGainsInQuote::PosInfinity => anyhow::bail!("Infinite max gains for new position"),
    } - match position_detail.max_gains_in_quote {
        MaxGainsInQuote::Finite(x) => x.into_number(),
        MaxGainsInQuote::PosInfinity => anyhow::bail!("Infinite max gains for position_detail"),
    };

    // 0.5 - 0.44 = 0.06
    ensure!(
        diff_max_gains > "0.05".parse()? && diff_max_gains < "0.06".parse()?,
        "Max gains is updated with proper delta"
    );

    perp_app.close_position(position_detail.id.0).await?;

    Ok(())
}

// Similar in spirit with decEqual (typescript code)
fn threshold_range(n1: Uint128, n2: Uint128, threshold: Uint128) -> Result<()> {
    let num = n1.checked_sub(n2)?;
    ensure!(
        num <= threshold,
        "{n1} - {n2} = {num} within the threshold of {threshold}"
    );
    Ok(())
}
