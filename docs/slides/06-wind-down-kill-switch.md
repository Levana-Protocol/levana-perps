---
title: Wind down and kill switch
---
# Wind down and kill switch

* Two related features for distinct purposes
* Share an underlying mechanism
* Kill switch
    * Catastrophic event
        * Bug in code
        * Extreme market conditions
        * Blockchain failure
    * Need emergency shut down of some part of the system
* Wind down
    * Turning off a market for some reason
        * New version of the contracts
        * Concerns over an asset
    * Slower, planned migration/deprecation
    * Need to release all liquidity to LPs
---
# Multisig requirements

* Highly security sensitive topics
* Actions must be mitigated by a multisig wallet
* Multisig nature is outside scope of perps
* Perps recognizes a kill switch wallet and a wind down address
    * Could theoretically be the same address
* Intend is to use a CW3 in production
---
# Factory versus market

* Factory contract maintains all auth information for markets
* Factory maintains "shutdown status" per market
* Each market queries the factory to determine its own shutdown status
* Convenience execute messages on factory to update many markets at once
---
# Shutdown impact

* Different items that can be shut down individually, e.g.
    * New trades (no open or update positions)
    * Deposit liquidity
    * Setting price
* Each impact can be disabled (e.g. cannot start new trades) or enabled (normal behavior)
* Allows for granular control
---
# Close all positions

* Only used for market wind down
* Indicates to the crank that any open positions should be closed
* Allows liquidity providers to withdraw any remaining liquidity
* Intended to be used after shutting down new trades
---
# Kill switch procedure

Not mitigated by the contracts, guidelines to the multisig participants:

* Should only be used for catastrophic events
* Shut down the impacted areas of the protocol
* If uncertain, shut down the entire protocol
* In the future, may require a DAO vote to reopen the protocol
---
# Market wind-down procedure

* Identify a need for a wind down (asset concern, migrating to new contracts, etc.)
* Announce plans to the world with timelines
* Shut down ability to deposit liquidity and open new position
* After some time, close all open positions
* After some more time, may stop updating price and turning crank in the market
