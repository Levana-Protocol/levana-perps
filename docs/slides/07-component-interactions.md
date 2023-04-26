---
title: Component interactions
---
# Component interactions

Multiple components make up the entirety of the perps platform.

* Smart contracts
    * Next slide deck will analyze the contracts themselves
* Frontend
* Price server
* Indexer
* Crank bot
* Price bot
* Monitoring/alerting
---
# Data storage

* Wherever possible, we store data on blockchain and do not rely on off-chain indexing
    * Reduces centralization of platform
    * More resilient to systems outages
* Indexing may be used in the future to cache some of this data for optimizing frontend
* Data that cannot be calculated on chain should be calculated by indexer
* Smart contract responsible for emitting events
* Events can be processed by indexer or other tools
---
# Indexer workflow

* Database: PostgreSQL database using tables with JSONB columns
* Ingestor: finds all transactions against perps contracts and inserts into tables
    * Also detects failed transactions and stores them to detect changes in failed transaction rates
* Processor: performs batch calculations and stores results in database
    * Example: calculate APRs for LPs by computing yields and average locked collateral over a time period
* REST API: provides frontend access to these data points, including historical LP chart data
* Analytics dashboard: internal tool that queries the database directly
---
# Monitoring/alerting

* Still a work in progress
* Aggregating data from indexer into a central dashboard
* Working on details for mainnet of which alerts will be raised and how
---
# Price server

* Captures second-by-second data from an external source (currently Binance API)
* Stores raw price data
* Generates candlestick data for frontend display
* Provides an endpoint to query current price by asset pair
---
# Price updates: today

* Each market recognizes a single address as the price setter
* Address is a hot wallet controlled by the price bot
* Price bot periodically (every ~60 seconds) queries price server for latest price
* Price bot makes privileged called to market to update the price

Not ideal, totally centralized, leverages hot wallet, significant security implications.
---
# Oracle-based price updates

* (Potentially) change price server's data source to a third-party oracle's data feed
    * Will be used for candlestick data
* Write a new smart contract which can
    * Get current price from on-chain oracle
    * Submit transaction to market to update price
* New smart contract will allow anyone to perform the price update
* Market will only recognize this smart contract for performing price updates
* We'll continue to run a price bot to periodically push price updates
* Crank incentivization will provide rewards for others to perform price updates

Fully decentralized, thread model is based on third-party oracle, not Levana services.
