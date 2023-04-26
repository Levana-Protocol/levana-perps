---
title: Introduction
---
# Levana Well Funded Perpetuals

* Platform explanation via slides
* Intended for use in audit process
* May be expanded in the future
---
# Other documentation references

* [High level overview](https://www.notion.so/levana-protocol/Levana-Well-funded-Perpetuals-high-level-overview-cd53b3d817c746cfb2d9041d8ccbf336)
* [Whitepaper](https://www.notion.so/Levana-Well-funded-Perpetuals-Whitepaper-9805a6eba56d429b839f5551dbb65c40)
* [Auto-generated message docs](http://levana-dev-docs.s3-website.ap-northeast-2.amazonaws.com/levana-perps-multichain/docs/api/cosmos/msg/doc/levana_perpswap_cosmos_msg/)
---
# What is Levana Perps?

* Perpetual futures/swaps platform
* Focused on _well fundedness_: if implemented correctly, platform can never become insolvent
* Support for small cap tokens
---
# Differences from common perps platforms

More details on all these points in later slides

* Locked liquidity
* No mark price
* Linear calculation of many values (e.g., funding fees, borrow fees, xLP unstaking)
* Crypto-denominated markets
* NFT-based positions (positions are non-fungible due to parameters like max gains)
* Ability to update parameters of a position
---
# Intended users

* Traders
    * General price speculation
    * Hedging positions in other platforms
* Liquidity providers
    * Low-risk gains regardless of market direction
    * Assume protocol and market windup-winddown risks
* DAO/protocol: maintain system, profits through trading fees
* Arbitrageurs
    * Cash-and-carry to balance interest and receive fees
---
# High level system interaction overview

*Liquidity provider*

* Deposits collateral asset in the pool, receives LP tokens
* Receives borrow fee payments from traders
* Can stake LP into xLP for higher returns
* Yields can be collected or reinvested
* Unstake xLP into LP over a 21 day period
* Withdraw LP into collateral
---

# High level system interaction overview

*Trader*

* Opens position, locks in liquidity from pool for max gains
* Pays fees on position open (trade fee, delta neutrality)
* Pays ongoing fees for position maintenance (borrow fee, funding payment, crank fee)
* Can update positions to add/remove collateral (changing exposure or leverage), alter max gains, or increase/decrease leverage
* View status of the position: PnL, calculated leverage, etc.
* Close position:
    * Manually
    * Liquidation/max gains
    * Trigger order (stop loss)
* Can open a position with a limit order instead of market
---
# High level system interaction overview

*Price setter*

* Privileged API call into the market to set the price
* Can be a hot wallet, hardware wallet, or a smart contract
* Current version: hot wallet from a privileged service we run grabbing data from Binance API
* Future: rely on an on-chain oracle, provide an external smart contract with privileges to do price updates
* Incentivize price updates via the crank mechanism (next slide)
---
# High level system interaction overview

*Crank turner*

* Scheduled tasks, price triggers, etc. run on the crank
* Crank turner checks if work is available and "turns the crank"
* Crank fees collected from various operations
* Crank turner receives those fees (if available)
* Price setting involves a crank, so price setter will receive crank fees
* Allows for
    * Decentralized platform (no need for a central authority to crank)
    * Prevents various attack vectors from forcing gas fees on others
---
# Well fundedness

* All potential gains for a trader are locked in the position
* All potential fees incurred by a user are part of the liquidation margin
* Position liquidated as soon as price movement would encroach on liquidation margin
* Position "max gained"/"take profited" when all potential gains are achieved
---
# Basic terminology guide

*Sample markets*

* `ATOM_USD`: speculate on ATOM vs USD, use ATOM as collateral
* `OSMO_USDC`: speculate on OSMO vs USDC, use USDC as collateral
* `ETH_BTC`: speculate on ETH vs BTC, use BTC as collateral

*Five named assets*

* Base asset: what the user is speculating on: ATOM, OSMO or ETH
* Quote asset: what the base asset is priced in: USD, USDC, or BTC
* Collateral asset: what traders use to open positions and liquidity providers send to the platform: ATOM, USDC, or BTC
* Notional asset: which asset internally is used for position size: USD, OSMO, or ETH
* USD: many values are always given in terms of USD, e.g. PnL

More details when we discuss collateral-is-base markets.
---
# Position attributes

* Deposit collateral: how much a trader deposited to open a position
    * This includes deltas from position updates
* Active collateral: current trader collateral after paying fees and realizing price exposure
* Counter collateral: liquidity pool liquidity locked in position, represents the max gains amount
    * Max gains can also be talked of as a percentage, more details later
* Max gains price: price point when trader will win all counter collateral
* Liquidation price: price point when trader will have insufficient funds to cover liquidation margin
* Notional size: system's internal representation of the exposure of the position
* Leverage: notional size versus active collateral
---
# Net notional/delta neutrality

* Net notional == sum of long notional sizes - sum of short notional sizes
* Open interest == sum of long notional sizes + sum of short notional sizes
* Delta neutrality ratio: `net_notional * price / pool_size`
* Protocol goal: keep net notional close to 0 (aka delta neutral)
