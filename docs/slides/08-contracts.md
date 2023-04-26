---
title: Contracts
---
# Contracts

* Perps platform consists of multiple contracts
* Some contracts are testnet-only, can be ignored for audit
* Overall design philosophy is _monolith_: minimize total number of contracts
    * Downside: larger contract size, beyond the default 800kb limit on Cosmos
    * Upsides
        * Arguably simpler code
        * Less cross-contract calls, leading to lower gas
---
# External dependencies

Primary perps protocol dependencies are:

* Some source for price information
    * Will move to an oracle when available
    * See crank slide deck for more details
* Asset to be used as collateral within the contract
    * Support for both native coins and CW20s
    * Can use IBC coins
---
# Testnet-only contracts

*Tracker*

* Keeps track of all code IDs deployed, which contract they represent, Git SHA they come from
* Keeps track of historical deployments of contracts
* Ties in with the deploy tool
* Allows for easy discoverability of frontend during testing
* Allows for better debugging to know what contract code is running
---
# Testnet-only contracts

*cw20*

* Mostly standard CW20 contract
* Used for providing collateral asset on testnet
* Includes some special functionality for trading competition to transfers
    * Trading competition restriction: cannot send funds between different wallets
    * Ensures "no cheating" during the competition
---
# Testnet-only contracts

*Faucet*

* Distributes CW20 collateral asset and gas coins to users
* Off-chain backend server taps the faucet on behalf of users
* Privileged wallets can mint large numbers of collateral assets
    * Used for additional testing and automated bots
---
# Mainnet contract overview

* Factory: one per entire protocol
    * Authentication
    * Instantiates all other contracts
* Market: one per market
    * Provides all core functionality in the system
    * Tracks positions, LP and xLP balances
* Liquidity token proxy
    * Provides a CW20 interface
    * Deployed by factory when deploying a new market
    * Two instantiations per market: LP and xLP
    * Allows trading LP/xLP in other systems like Osmosis Zone
* Position token proxy
    * Provides a CW721 (NFT) interface
    * Allows traders to buy and sell positions on secondary markets
    * Can work with any marketplace supporting NFTs
---
# Factory

* Contains a list of special addresses
    * Owner (can add new markets)
    * Wind down address
    * Kill switch address
    * Etc
* Contains list of markets and their contracts
* Instantiates fresh market, liquidity token, and position token contracts
* Migrates existing contracts
---
# Market

* Vast majority of logic lives here
* When necessary, accesses data from factory using raw queries
* Provides CW20 and CW721 proxy interface for liquidity and position token contracts
* Depends on a token source for collateral, either:
    * Native coin (including IBC)
    * CW20
---
# Not yet implemented: price setter contract

* Not part of the primary perps codebase
* Will rely on external price oracle
* Recognized as the price setter for a market
* Allows non-permissioned setting of price within the market
* Plan is to pass along crank rewards to caller
