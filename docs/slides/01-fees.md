---
title: Fees
---
# Fees within the protocol
---
# Protocol tax

* Amount of the fees sent to DAO versus LPs
* Applies to
    * Borrow fee
    * Trade fee
---
# Borrow fee

Also known as "cost of capital," still updating docs

* Paid by traders to liquidity providers
* Accrued on a linear, consistent basis
* Borrow fee rate determined by delta from target utilization ratio
    * Utilization ratio == `locked liquidity / total liquidity`
    * If `actual < target`, reduce borrow fee rate
    * If `actual > target`, increase borrow fee rate
* Borrow fee is paid into yields, _not_ directly into the pool
* Liquidity providers/DAO can collect these funds regardless of liquidity locking status
    * Will discuss more when discussing liquidity pools
---
# Trade fee

* Charged on position open and update
* Two components
    * Percentage of notional size of position
    * Percentage of locked counter collateral
* Charged on updates only when those numbers go up, not down
    * Removing collateral to reduce position size: no charge
    * Adjusting leverage up: charged
    * Reducing max gains percentage: no charge
---
# Crank fee

* Charged for actions that require the crank to run
    * More details in later slide deck on what causes a crank
* Positions that schedule cranks for later must reserve sufficient collateral to pay fee
* Fees go into a crank fee fund
* Users who "turn the crank" receive portion of that fund on an as-available basis
* Generally crank charge is much greater than payout to disincentivize spam attacks
---
# Funding payments

* Like standard perps platform: popular positions pay unpopular
* However: funding payments are continually accrued, not a stepwise basis
* Provides an incentive to balance the protocol between longs and shorts
* Opportunity for arbitrage/cash-and-carry bots
---
# Delta neutrality fee

Also known as artificial slippage, still updating docs and code

* Similar in concept to funding payments
* However: one-time fee at position open, update, and close
* Pay to the fund when moving us farther from neutral
* Receive a reward when moving us closer to neutral
* Intended to hinder price manipulation attacks
* Provides more immediate feedback than longer-term funding payments
