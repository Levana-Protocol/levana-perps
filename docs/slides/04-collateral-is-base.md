---
title: Collateral is base markets
---
# Collateral is base markets

* Previously called "flipped" markets
* Use the base asset as collateral
* Allows us to bypass need for stablecoins yet have USD-priced markets
* Internally, the system needs to "flip" notional to be the quote asset (more details coming)
    * Base asset is collateral
    * Quote asset is notional
* Protocol treats this "flipping" as an internal detail
* We'll use `ATOM_USD` market for consistent example
---
# Market IDs

* All market IDs are given as `BASE_QUOTE`
* Fiat currencies cannot be used as collateral since they do not exist on the blockchain
* If the quote asset is fiat (USD or EUR), then we assume we're in a collateral-is-base market
* Otherwise, defaults to collateral-is-quote
* Can use a plus sign to indicate a collateral-is-base override

| Market ID | Collateral Type | Base | Quote | Notional | Collateral |
| --- | --- | --- | --- | --- | --- |
| ATOM_USD | Base | ATOM | USD | USD | ATOM |
| OSMO_USDC | Quote | OSMO | USDC | OSMO | USDC |
| ETH_BTC | Quote | ETH | BTC | ETH | BTC |
| ETH+_BTC | Base | ETH | BTC | BTC | ETH |
---
# Price conversions

* `price_base`: price of base asset (ATOM) in terms of quote asset (USD)
    * E.g. 10 USD per ATOM
    * This is the "standard" spot price
    * Uses the data type `PriceBaseInQuote`
* `price_notional`: price of notional asset (USD) in terms of collateral asset
    * E.g. 0.1 ATOM per USD
    * This is valid, but not the way people generally think of assets
    * Uses the data type `Price`
* `price_collateral`: used for USD PnL calculations, always gives price of collateral in USD
    * In `ATOM_USD` market, happens to be the same as `price_base`
    * In `OSMO_USDC`, would be price of USDC in USD (generally: 1 USD per USDC)
    * In `ETH_BTC`, would be price of BTC in USD (e.g. 22,000 USD per BTC)
---
# Longs become shorts

* Going long on ATOM_USD means: I think ATOM will go up in value versus USD
* If ATOM becomes more valuable, USD becomes less valuable
* Could consider equivalent to going short on USD_ATOM
* Similarly, short on ATOM_USD == long on USD_ATOM
* We'll use positive leverage to represent longs, negative to represent shorts
---
# Exposure

* Perps platform provides _exposure_ to the base asset
* Simple example: spot market
    * Buy 10 ATOM for 100 USD
    * Price of ATOM goes up 20%
    * Sell 10 ATOM
    * Made 20% gains on our USD (100 USD to 120 USD)
* This is a 1x leverage long
* Perps allows for higher leverage and shorting
* Next slides: demonstrations of leveraged exposure, then back to collateral-is-base markets
---
# Long leveraged exposure

* Price of ATOM is 10 USD, I have 100 USD
* _Could_ buy 10 ATOM
* Instead: with 5x leverage I'm _exposed_ to 50 ATOM
* Price goes up to 12 USD (20% gain)
* Value of 50 ATOM goes to 600 USD
* Original value was 500 USD
* Trader makes 100 USD gains on the trade
* Versus spot 1x market, gains are 5x higher
---
# Short leveraged exposure

Works in same way for shorts

* Price of ATOM is 10 USD, I have 100 USD
* Open 5x short position on ATOM, exposed to 50 ATOM
* Price goes up to 11 USD (10% price movement against me)
* Multiply price movement (-1 USD/ATOM) by exposure (50 ATOM)
* Losses on position: -50 USD
---
# Collateral is base: flip the direction

* In collateral-is-base, we use base as collateral, quote as notional
* Going long on ATOM == going short on USD
* If price of ATOM in terms of USD goes up, price of USD in terms of ATOM goes down
* Let's rework our long leveraged exposure in USD in terms of ATOM
---
# Going leveraged short on USD

* Price of USD is 0.1 ATOM, I have 100 USD
* Since ATOM is collateral, buy 10 ATOM with 100 USD
* Use 10 ATOM as deposit collateral on my position
* Choose a 5x leveraged short, get exposure to 500 USD
* Price goes down to 1/12 == 0.833 ATOM per USD, 20% dip
* New value of 500 USD is 41.666 ATOM
* Price moved in my direction by 50 ATOM - 41.666 ATOM == 8.333 ATOM
* Close position, extract original 10 ATOM + 8.333 ATOM PnL
* Sell 18.333 ATOM at current market price (12 USD per ATOM) == 220 USD
* Gained 120 USD instead of 100 USD

Why is the outcome different from going long on ATOM?
---
# Off-by-one exposure

* When using base asset as collateral, forced to hold some of the speculated asset
* Automatically get some exposure to asset through that
* Need to adjust our leverage
* Using our signed leverage, we express this as: `leverage_to_notional = 1 - leverage_to_base`
* 5x long on ATOM == 4x short on USD
* 5x short on ATOM == 6x long on USD
* Let's redo our leveraged short example using 4x leverage instead
---
# Going leveraged short on USD, fixed

* Price of USD is 0.1 ATOM, I have 100 USD
* Since ATOM is collateral, buy 10 ATOM with 100 USD
* Use 10 ATOM as deposit collateral on my position
* Choose a **4x** leveraged short, get exposure to 400 USD
* Price goes down to 1/12 == 0.833 ATOM per USD, 20% dip
* New value of 400 USD is 33.333 ATOM
* Price moved in my direction by 40 ATOM - 33.333 ATOM == 6.666 ATOM
* Close position, extract original 10 ATOM + 6.666 ATOM PnL
* Sell 16.666 ATOM at current market price (12 USD per ATOM) == 200 USD
* Gained 100 USD like the original 5x long on ATOM
---
# Signed leverage examples

* Trader expresses absolute value of leverage and direction, e.g. 5x short
* We convert to _signed leverage to base asset_, e.g. -5
* Then we convert to _signed leverage to notional asset_
    * Collateral is base: `1 - -5 == 4`
* Internally, store a notional size in notional asset levered by signed leverage to notional, e.g.
    * Deposit collateral == 100 ATOM
    * Notional size in collateral == 100 ATOM * 4 == 400 ATOM
    * Notional size = 400 ATOM / price_notional (0.1 ATOM/USD) == 4000 USD
* By contrast, if you want to go 7x long on ATOM with 100 ATOM collateral
    * Signed leverage to base == `7`
    * Signed leverage to notional == `1 - 7` == `-6`
    * Notional size in collateral == 100 ATOM * -6 == -600 ATOM
    * Notional size = -600 ATOM / price_notional == -6000 USD
---
# Calculating current leverage

* Leverage changes over time due to fees and price movements
* Trader cares about leverage to base, not to notional
* System can only directly calculate leverage to notional

*Calculation process*

* Notional size of position is fixed, e.g. -6000 USD
* Active collateral is 60 ATOM
* Current price: 10 USD per ATOM
* Notional size in collateral: -6000 USD / 10 USD per ATOM == -600 ATOM
* Signed leverage to notional == -600 ATOM / 60 ATOM == -10 (e.g. 10x short)
* Convert to base: `1 - -10` == `+11` == 11x long to base
---
# Max gains in quote

* Max gains are determined by locked counter collateral
* Traders consider the base asset volatile, quote asset stable
* Want to express maximum gains as a percentage of the _quote asset_
* Comparing:
    * How much quote asset would I have if I sold my collateral at entry price?
    * How much quote asset would I have if I won all the counter collateral and sold at the final max gains price?
* Example:
    * Entry price: 10 USD/ATOM. Final price: 15 USD/ATOM
    * Deposit collateral: 100 ATOM. Counter collateral: 200 ATOM.
    * Initial collateral value: 100 ATOM * 10 USD/ATOM == 1000 USD
    * Max gains: take 300 total collateral from protocol
    * Value in USD: 300 ATOM * 15 USD/ATOM == 4500 USD
    * Gains: 4500 USD - 1000 USD (investment) == 3500 USD
    * Gains percentage in quote: 3500 USD / 1000 USD == 3.5 == 350%
---
# Infinite max gains

* Only applies to longs in collateral-is-base markets
* Speculating on price of base/collateral asset going up
* As price goes up, winning more counter collateral, _and_ value of the winnings goes up
* With large enough counter collateral, can have unbounded maximum gains in terms of quote
* This occurs when counter collateral == notional size in collateral
