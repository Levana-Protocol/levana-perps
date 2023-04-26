---
title: Trading overview
---
# Trading overview

* Levana perps support two market types
* Collateral is quote: use the same asset to price the base asset as for collateral
    * Example: `OSMO_USDC`. Deposit OSMO, price of OSMO reflected in terms of USDC
* Collateral is base: use the base asset for deposits
    * Example: `ATOM_USD`. Deposit ATOM in the system, use that to speculate on price movements of ATOM
* Collateral is quote is the common perps paradigm, and what we'll discuss in these slides
* Next slide deck will cover collateral is base modifications
---
# Position open parameters

* Deposit collateral: funds the trader sends into the system
* Leverage: multiplier on deposit collateral for notional size in collateral. Example:
    * Deposit: 1000 USDC
    * Leverage: 5x
    * Price of OSMO: 2 USDC
    * Notional size: 1000 USDC * 5 / 2 (USDC/OSMO) == 2500 OSMO
* Max gains in quote: percentage of gains trader can experience versus deposit
    * With max gains of 300%, 1000 USDC deposit leads to 3000 USDC
    * This becomes the _counter collateral_ and is locked from the liquidity pool
    * Position cannot be opened if there is insufficient unlocked liquidity
---
# Triggers and limits

* Limit order: only open the position when the price moves to that point
    * Longs: if the spot price is less than the trigger price
    * Shorts: if the spot price is greater than the trigger price
* Stop loss: trigger on a position causing position to close before liquidation point
    * Internally we support a take profit price as well, but UI does not expose it
    * Better for traders in general to reduce max gains than set a take profit price
    * Reduced max gains == lower borrow fee
* Triggers on open positions can be updated
* Triggers and limits managed by crank system
---
# Liquidation margin

* Positions can accrue fees and may owe more fees at closing
    * Borrow fee incurred entire time position is open
    * Funding payments _may_ need to be paid (may also be received)
    * Cranks need to be paid for
    * Delta neutrality fee may be incurred on close (may also be received)
* Liquifunding process calculates and pays these fees
* Liquifunding is scheduled to run regularly for a position (currently: every 24 hours)
* Have an additional "staleness" period (currently: every 2 hours)
* Liquifunding delay + staleness period (26 hours) == liquifunding duration
* At position open, update, and liquifunding, ensure we have enough funds for maximum of all payments above
* Necessitates max payments for all four fees
---
# Position open validation process

* Calculate notional size, counter collateral, etc. from parameters
* Perform slippage validation (described below)
* Validate that leverage is within range
* Validate that minimum deposit is met (avoids spam attacks)
* Charge trading fee from active collateral
* Lock up counter liquidity
* Charge delta neutrality fee
* Adjust protocol net notional
* Check liquidation margin is met
* Store liquidation/max gains trigger prices for crank
---
# Delta neutrality fee

* Sensitivity parameter based on trade volume size of spot market for asset
* Larger spot market == larger sensitivity value == smaller fees
* Caps indicate how far from neutral we can go
    * Enforced on position open and update
    * Traders can always close positions, plus liquidations can happen
    * In those cases, we may go beyond the cap
* The farther from neutral we go, the higher the fee is
* Incentivizes traders to open smaller positions and wait for arbitrageurs to balance protocol
---
# Slippage tolerance

* Spot price may move between trader initiating an open and transaction landing
* Net notional may move leading to different delta neutrality fee
* Optional slippage tolerance allows a position to be canceled if price or net notional moves too much
---
# Staleness

* Liquifunding must occur regularly on a position
* Without it, cannot guarantee well fundedness
* Levana currently runs crank bots looking for new work items regularly
* Crank fee provides incentives for others to run crank bots too
* This mechanism could fail
    * Blockchain could go down
    * DDoS attack takes down all bots
    * Operations failure (servers down, insufficient gas funds, etc.)
* If any position crosses into stale territory, entire protocol becomes stale
* Impact: cannot open, update, or close positions
* Cranking will handle queued work items in order and un-stale the protocol
---
# Liquifunding process

* Pay fees accrued since last liquifunding
* Calculate price exposure based on price delta since last liquifunding
    * Price moves in favor of trader: reduce counter collateral, increase active
    * Price moves against trader: reduce active collateral, reduce counter
* Adjust locked liquidity within pool based on price exposure impact
* Calculate new liquidation margin
* If active collateral <= liquidation margin, or counter collateral <= 0, close position (liquidation or take profit, respectively)
* Otherwise: schedule next liquifunding, store new liquidation/take profit price triggers for crank
---
# Position update actions

* Add collateral, keep notional size same, decrease leverage
* Add collateral, keep leverage same, increase notional size
* Remove collateral, keep notional size same, increase leverage
* Remove collateral, keep leverage same, decrease notional size
* Modify leverage (increase or decrease), notional size gets bigger or smaller
* Modify max gains (increase or decrease), counter collateral gets bigger or smaller
---
# Basic update process

* Validate all parameters
* Perform a liquifunding
* Make sure sufficient funds available for new liquidation margin
* Update locked liquidity, notional interest within protocol
---
# Close position process

* Validate parameters (just slippage tolerance)
* Perform liquifunding (may result in liquidation/take profit)
* Charge delta neutrality (plus or minus to active collateral)
* Adjust open interest for closing of position
* Unlock remaining counter collateral in pool
* Transfer active collateral to trader
* Remove from open positions, store in closed positions
* Remove liquidation price triggers
