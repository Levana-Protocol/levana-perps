# Table Of Contents

<!--toc:start-->
- [Table Of Contents](#table-of-contents)
- [levana-perps-qa](#levana-perps-qa)
- [Installation](#installation)
- [Contract Addresses and variables](#contract-addresses-and-variables)
- [Web Interface](#web-interface)
- [Usage](#usage)
  - [Deposit Liquidity](#deposit-liquidity)
  - [Check balances](#check-balances)
  - [Check total Open positions](#check-total-open-positions)
  - [Open Position](#open-position)
  - [Setting new price](#setting-new-price)
  - [Fetching latest price](#fetching-latest-price)
  - [Fetch Open positions](#fetch-open-positions)
  - [Fetch Closed postions](#fetch-closed-postions)
  - [Close position](#close-position)
  - [Crank](#crank)
  - [Get position details](#get-position-details)
  - [Tap faucet](#tap-faucet)
  - [Update Leverage](#update-leverage)
  - [Update Collateral](#update-collateral)
    - [Increase Collateral](#increase-collateral)
    - [Decrease Collateral](#decrease-collateral)
  - [Update Max Gains](#update-max-gains)
- [Tutorial](#tutorial)
  - [Open simultaneous positions](#open-simultaneous-positions)
<!--toc:end-->

# levana-perps-qa

QA tools for the perps project

# Installation

- Download the binary for MacOS from [releases page](https://github.com/Levana-Protocol/levana-perps-qa/releases). We currently
  only provide binary for MacOS.
- Once you download the binary, do the following steps on your MacOS
  machine:

``` shellsession
mv perps-qa-apple-darwin perps-qa
chmod a+x perps-qa
xattr -d com.apple.quarantine perps-qa
```

Now put the binary on the `$PATH`. This can be done by something like
this:

``` shellsession
sudo mv perps-qa /usr/local/bin/perps-qa
```

# Contract Addresses and variables

Contract addresses are discovered using the same mechanism as the web frontend.
Contracts are identified by their _contract family_.  We have specific contract
addresses for QA purposes called `osmoqa`. This is the default contract family,
so you don't have to change it unless you want to test it with some other
specific contract.

That being said, you still need to set the following environment
variable:

- COSMOS_WALLET

The above environment variable contains the mnemonic phrase for the
wallet that allows you to set new price. You have to export it like this:

``` shellsession
export COSMOS_WALLET="REDACTED"
```

To get the wallet mnemonic for the QA contract which has the ability
to set price, contact admin.

You can use a different contract family by running:

``` shellsession
export LEVANA_PERP_CONTRACT_FAMILY=osmodev
```

# Web Interface

Since, we are using a separate contract for testing QA - you would
have to change the address provider of the [perpetual swaps
application](https://levana-web-app-develop.vercel.app/en/trade/USD_ATOM) via the following steps:

- Click the Ellipsis (or the Meatballs menu which is represented as
  three dots) in the home page.
- Replace the Address Provider with the factory address.

To find out the factory address, issue the following command:

``` shellsession
❯ perps-qa --verbose print-balances
[2023-01-04T08:22:21Z DEBUG levana_qa] Factory address: osmo13x3vrahfjguhm3mxyath9rfxuvemtl25ltc99rdtc77vcu7g5hrs5mv6l4
Wallet address: osmo1ssnwszx2dutvlrpwwgefpfyswgrrm059d5mcfe
99609046uosmo
Cw20 Balance: 1440933961
```

The first line contains the factory address details.

# Usage

## Deposit Liquidity

If you want the ability to open positions etc, then you would have to
fund the market initially. This can be done via:

``` shellsession
perps-qa deposit-liquidity
```

## Check balances

You can check your wallet's balance using the following command (Make
sure that the environment variable `COSMOS_WALLET` is exported):

``` shellsession
perps-qa print-balances
34833630udragonfire
Cw20 Balance: 2620002985000000
```

## Check total Open positions

``` shellsession
perps-qa total-position
Total Open positions in Contract: 3
```

## Open Position

Let's say we want to open position with the following parameters:

- Collateral: 5 USD
- Leverage: 4x
- Max Gains: 14%
- The current price is: 9.61 USD
- Max slipage: 1%

This is how you would do using the CLI:

``` shellsession
perps-qa open-position --collateral 5 --leverage 4 --max-gains 14 --current-price 9.61 --max-slippage 1
Transaction hash: 91802971217D56B03F278EA98CB70CA8808170B1580DB978301507F188029C2A
```

By default, it opens a long position. If you want to open a short
position instead with the same parameters, pass the `--short` flag:

``` shellsession
perps-qa open-position --collateral 5 --leverage 4 --max-gains 14 --current-price 9.61 --max-slippage 1 --short
Transaction hash: 91802971217D56B03F278EA98CB70CA8808170B1580DB978301507F188029C2A
```

## Setting new price

Let's say you want to set a new USD price of `9.43343`, then you would
have to do this:

``` shellsession
perps-qa set-price --price 9.4334933
Transaction hash: 203CA767446FE719B32962A2676152AD8B4ECF5DBEAF772137D395B0DBAC1286
```

## Fetching latest price

``` shellsession
perps-qa fetch-price
Latest price: 9.4334933
Updated at (UTC): Mon, 12 Dec 2022 10:08:10 +0000
Updated at (Local TZ): Mon, 12 Dec 2022 15:38:10 +0530
```

## Fetch Open positions

``` shellsession
perps-qa all-open-positions
3 Open Positions in this wallet: [2, 4, 5]
Total long positions: 1
Total short positions: 2
```

## Fetch Closed postions

``` shellsession
perps-qa all-close-positions
3 Closed positions: [0, 1, 3]
```

## Close position

You can close a position, by passing the position id:

``` shellsession
perps-qa close-position --position-id 3
```

## Crank

Sometimes, the work would have been queued. In which case, you would
have to crank:

``` shellsession
perps-qa crank
[2022-12-15T10:42:55Z INFO  levana_qa::contract] Cranking finished
```

## Get position details

``` shellsession
perps-qa position-detail --position-id 2
❯ perps-qa position-detail --position-id 0
Collateral: 5
Leverage: 4.009333294565754658
Max gains: 0.140326665309801407
Liquidation Price: 7.089084097164302012
Profit price: 9.763665565500000037
```

## Tap faucet

``` shellsession
perps-qa tap-faucet
Transaction hash: 0E6C0986A227FDF722B10BD7B60704DE0D3C8A363C351EADDF9C239B850E07A9
```

## Update Leverage

Let's get the details of the position that we want to update:

``` shellsession
❯ perps-qa position-detail --position-id 6
Collateral: 5
Active Collateral: 4.984492753623188407
Direction : long
Leverage: 4.009333294565754658
Max gains: 0.101764894019131799
Liquidation Price: 7.154092175483302126
Profit price: 9.853199999999999985
```

So the leverage is around 4. Let's try to increase it to 8 ATOM. We
will do this to achieve it:

``` shellsession
❯ perps-qa update-leverage --position-id 6 --leverage 8
Transaction hash: 474712EB5B0976639EF83DF297CAAA796A3529D87FCB6C7B4E8F2898ED8BC1D5
```

And now you can check again the position details to confirm if the
collateral has actually increased:

``` shellsession
❯ perps-qa position-detail --position-id 6
Collateral: 5
Active Collateral: 4.963932339783799391
Direction : long
Leverage: 8.028998972479095146
Max gains: 0.237695617426829298
Liquidation Price: 8.344242601736291478
Profit price: 9.853199999999999985
```

## Update Collateral

### Increase Collateral

Let's check the actual collateral of the position id that we want to
update:

``` shellsession
❯ perps-qa position-detail --position-id 7
Collateral: 5
Active Collateral: 4.984492753623188407
Direction : long
Leverage: 4.009333294565754658
Max gains: 0.101764894019131799
Liquidation Price: 7.154092175483302126
Profit price: 9.853199999999999985
```

Let's change it to 8 now. Do this to achieve it:

``` shellsession
❯ perps-qa update-collateral --position-id 7 --collateral 8.0 --impact leverage
[2023-01-04T05:18:00Z INFO  levana_qa::contract] Increasing the collateral
Transaction hash: 55D0E1CA2A4DCF43A8201D8F4CE0852DFDB47E6EB20417E22B1274BE39E850DE
```

You can verify that the collateral has indeed increased:

``` shellsession
❯ perps-qa position-detail --position-id 7
Collateral: 8.015507
Active Collateral: 8.000001524943820731
Direction : long
Leverage: 2.874999642591360144
Max gains: 0.063405785015166766
Liquidation Price: 6.216143378733786871
Profit price: 9.853199999999999985
```

### Decrease Collateral

Let's check the actual collateral of the position id that we want to
update:

``` shellsession
❯ perps-qa position-detail --position-id 7
Collateral: 8.015507
Active Collateral: 8.000001524943820731
Direction : long
Leverage: 2.874999642591360144
Max gains: 0.063405785015166766
Liquidation Price: 6.216143378733786871
Profit price: 9.853199999999999985
```

Let's try to change it from 8 to around 7 using the leverage impact:

``` shellsession
❯ perps-qa update-collateral --position-id 7 --collateral 7 --impact leverage
[2023-01-04T05:28:55Z INFO  levana_qa::contract] Decreasing the collateral
Transaction hash: 7C47BDB54A9F6F7478465FE74E8DB18CCA7087D8561E7A0EA7F5B349803808D5
```

And you can verify the same:

``` shellsession
❯ perps-qa position-detail --position-id 7
Collateral: 7.015505475056179269
Active Collateral: 6.99999307441180901
Direction : long
Leverage: 3.14285926293725805
Max gains: 0.07246383980947249
Liquidation Price: 6.498692295947909661
Profit price: 9.853199999999999985
```

## Update Max Gains

``` shellsession
❯ perps-qa position-detail --position-id 8
❯ perps-qa position-detail --position-id 8
Collateral: 500
Active Collateral: 499.467289719626168226
Direction : long
Leverage: 2.001066556892390023
Max gains: 0.065490335497632992
Liquidation Price: 4.768273372361801516
Profit price: 10.186399999999999964
```

Let's update the max gains above to 20 percentage:

``` shellsession
❯ perps-qa update-max-gains --position-id 8 --max-gains 20
Transaction hash: 9542D48EF06EA1C149602E5C3D18F9405FA83DA15A4F0071A22ECF04235C353B
```

Now you can confirm that both the Max gains and profit price would
have increase:

``` shellsession
❯ perps-qa position-detail --position-id 8
Collateral: 500
Active Collateral: 499.042842930031290555
Direction : long
Leverage: 2.001917985767211791
Max gains: 0.910790158997869954
Liquidation Price: 4.770359013594976067
Profit price: 104.66900794909349708
```

# Tutorial

## Open simultaneous positions

To open two positions at nearly the same time, you can create a
script and execute it. Open a file named `levana_script.sh` and have
these contents:

``` shellsession
#!/usr/bin/env bash

set -euo pipefail

perps-qa open-position --collateral 5 --leverage 4 --max-gains 14 --current-price 9.61 --max-slippage 1
perps-qa open-position --collateral 5 --leverage 4 --max-gains 14 --current-price 9.61 --max-slippage 2
```

Mark the script for execution:

``` shellsession
chmod +x levana_script.sh
```

And you can execute it like this:

``` shellsession
./levana_script.sh
```
