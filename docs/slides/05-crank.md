---
title: Crank
---
# Crank

General mechanism for "maintenance." Allows for:

* Scheduling tasks by time (e.g. liquifunding a position)
* Price triggers (e.g. liquidations, max gains, limit orders)
* Large batch operations that will take more than one transaction (e.g. resetting LP balances to 0)
---
# Price update gating

* Crank is driven by price updates
* Crank will fully process one price update before moving to the next
* Ensures that if a position was liquidated at an old price point, we close it before processing new price updates
---
# Overall operations for crank

* Query message: check if there are any work items
* Batch operate crank: perform up to N operations
    * Check if there is a work item
    * Do the work item
    * Repeat N times
* Explicit crank message: perform a batch operation
* Price updates: includes a batch crank by default since each price update requires some processing work
---
# Work items in crank

*We'll detail the actions in later slides*

* Check if the "close all positions" flag is set
    * Used in market wind down
    * If flag is set, get the next open position ID if available
* Is the "reset LP balances" flag set? If so, perform "reset LP balances"
* Are there any price updates that haven't completed cranking? If no, exit. If yes:
* Is there a liquifunding scheduled before that price update? Then liquifund that position
* Unpend any waiting liquidation prices queued up before that price update
* Have any liquidations, max gains, or triggers hit? If so, close the position
* Any limit orders hit? If so, open the position
* None of those triggered? Mark this price point as fully cranked
---
# Reset LP balances

* Find next LP or xLP token holder
* Accrue any yields they have earned by not collected
    * More details later in yield calculations explanation
* Delete all token holder information
* If no token holder information available: clear the "reset LP balances" flag
---
# Liquidations

*Intentionally going out of order, unpending occurs first, but we need to understand liquidations before that.*

* Use cw-storage-plus's `Map` to sort positions by trigger price
* Consider: new price point is $10
* We want to close all positions where:
    * Longs: liquidation price >= $10
    * Longs: max gains price <= $10
    * Shorts: liquidation price <= $10
    * Shorts: max gains price >= $10
* All liquidation prices are stored in `Map`s
* O(1) operation on each to check "is there a new position available"
* Applies to trigger prices (stop loss/take profit) as well
---
# Unpending: the need

*Scenario*

* Price update at 6am to $10, crank runs
* Price update at 6:10am to $20, crank does not run
* Price update at 6:20am back to $10, crank still does not run
* Trader opens short position at 6:21am, based on $10 entry price gets liquidation price of $15
* Crank begins to run, sees 6:10am price update
* Processes liquidations, sees that trader's position from 6:21am needs to be closed

This isn't correct! The price that caused the liquidation is _in the past_ from the trader.

How do we avoid this?
---
# Unpending: the solution

* When opening a position, generate new liquidation/max gains prices
* Place onto the "unpending" queue
* Do not process these prices until all previous price points have been cranked
* Only then: insert into the liquidation data structures

Unpending happens before liquidations to ensure our liquidation data structures are fully updated before we look for price triggers.
---
# Limit orders

* Virtually identical to the liquidation price concept
* Keep long and short data structures
* Longs: open all positions where limit price <= current price
* Shorts: open all positions where limit price >= current price
---
# Price update staleness

Lack of price updates represents a precognition risk

* Last price update to system == $10
* Attacker suspects price will go up
* Attacker launches DDoS attack to prevent price updates to be sent
* Attacker opens a highly leveraged long position
* Attacker observes price movement on spot actually goes up
* Attack stops DDoS, lets new price into system, realizes large PnL
* If price moves against attacker, closes position without letting new price into system

To prevent this: if the price has not been updated for a long enough period, we disallow opening/updating/closing positions.
---
# Crank lag staleness

If crank has not run long enough that a position is beyond its liquifunding staleness bound, illiquidity risk:

* Position A opens, sets aside margin for 26 hours of funding payments
* Position A ends up on popular side, accrues large funding payment debt
* Crank does not run for 50 hours, entire extra day of funding payments need to be made
* New trader opens unpopular position B, receives funding payments, closes position B and takes fees from system
* Crank runs, position A is liquidated, but insufficient funds to cover fees that position B already took

Therefore: protocol goes stale when crank is too far behind, funding payments on close do not include any time beyond last successful crank point.
