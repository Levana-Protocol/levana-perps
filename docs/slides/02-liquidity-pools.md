---
title: Liquidity Pools
---
# Liquidity pools

* Standard perps: peer-to-peer
    * Longs are betting against shorts
    * Large numbers of traders cashing out can impact liquidity and impact mark price
* Levana perps: peer-to-pool
    * Betting against the liquidity pool
    * Potential gains locked in when opening the position
* Traders benefit: guaranteed to have funds available to cash out
* Liquidity providers benefit: receive low-risk return
    * Risks discussed later
    * We'll say "providers" through rest of these slides for brevity
---
# Locked vs unlocked liquidity

* Deposit collateral, becomes unlocked liquidity
* Trader opens a position, becomes locked
* Providers cannot withdraw locked liquidity
* Trader closes position -> remaining liquidity is unlocked -> providers can withdraw

Rewards are _not_ sent to the liquidity pool so they remain liquid and providers can always withdraw them.
---
# Impairment

Scenario:

* Provider deposits 1000 USDC in pool, receives 1000 LP tokens
* Trader opens a position, deposits 250 USDC, locks 500 USDC
    * Max gains: 200%
* Trader achieves max gains and takes 500 USDC
    * Trader withdraws 750 USDC total
    * Liquidity pool loses 500 USDC
    * Each LP token now only worth 0.5 USDC
* Trader hits complete liquidation, loses all 250 USDC
    * Liquidity pool has 1250 USDC total
    * Each LP token worth 1.25 USDC

Goal is to minimize impairment
---
# Risks to providers

* Providers are forced to take opposite-side positions
* If protocol is delta-neutral, providers face 0 risk from price movement
    * Each price move causes exactly opposite effect on longs and shorts
* If protocol is not delta-neutral, pool faces risk from price movement
* Could result in negative impairment (money lost to traders) or positive impairment (money won from traders)
* Protocol goal is to incentivize delta neutrality (delta neutrality fee and funding payments)
* In extreme cases (like a market crash), significant impairment is expected
---
# LP vs xLP

* LP tokens can be immediately withdrawn whenever there is sufficient unlocked liquidity
* xLP is a longer term (21 day) staking
* Can deposit directly to xLP, or stake LP into xLP
* Guaranteed 1:1 ratio between LP and xLP tokens
* xLP can be unstaked to LP
    * Linear unstaking process
    * Can collected unstake LP at any point during that process
* LP and xLP tokens have CW20 interface, can be traded in AMMs
    * Allows for immediate liquidity exit if needed
---
# Normal case: LP-to-collateral ratio changes

* Scenario: 100 LP tokens backed by 100 USDC
* Trader locks 50 USDC in a position
* Trader experiences max gains, takes all 50 USDC from pool
* Pool now has 100 LP tokens and 50 USDC
* Each LP now backed by 0.5 USDC
* New provider deposits 100 USDC
* New provider needs to receive 100 / 0.5 == 200 LP tokens

Due to impairment, LP-to-collateral ratio can change over time.
---
# Degenerate case: all liquidity lost to impairment

* Scenario: 100 LP tokens backed by 100 USDC
* Trader locks all 100 USDC in a position
* Trader experiences max gains, takes all 100 USDC from pool
* Pool now has 100 LP tokens and 0 USDC
* Each LP now backed by 0 USDC
* New provider deposits 100 USDC
* New provider needs to receive 100 / 0 == infinite LP tokens???

Obviously doesn't work, what do we do?
---
# LP token reset

In degenerate case of all liquidity lost:

* Freeze the protocol
* Reset all LP token balances to 0
* Unfreeze the protocol
* Next person to deposit gets the default LP-to-collateral ratio of 1-to-1

Since the reset can take many transactions, we use the crank mechanism for this.
