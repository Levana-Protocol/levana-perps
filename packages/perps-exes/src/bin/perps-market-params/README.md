# perps-market-params

This tool aids in checking of market parameters. Currently it only
supports DNF sensitivity check.

# Usage

``` shellsession
❯ cargo run --bin perps-market-params -- --help
Usage: perps-market-params [OPTIONS] <COMMAND>

Commands:
  scrape        Scrape particular coin
  scrape-local  Scrape local file
  coins         List Supported coins with it's id
  dnf           Compute DNF sensitivity
  help          Print this message or the help of the given subcommand(s)

Options:
      --verbose
  -h, --help     Print help
```

# Coin List

Currently it only support limited set of coins. To see the supported
coins, run:

``` shellsession
❯ cargo run --bin perps-market-params -- coins
    Finished dev [unoptimized + debuginfo] target(s) in 0.30s
     Running `target/debug/perps-market-params coins`
2024-03-20T08:37:10.893559Z  INFO perps_market_params: Levana (id: levana)
2024-03-20T08:37:10.893597Z  INFO perps_market_params: Atom (id: cosmos-hub)
```

Note that you would have use the id of the coin to specify the coin
type.

# DNF computation

To see DNF computation of levana coin:

```
❯ cargo run --bin perps-market-params -- dnf --coin levana
    Finished dev [unoptimized + debuginfo] target(s) in 1.09s
     Running `target/debug/perps-market-params dnf --coin levana`
2024-03-20T08:36:45.744311Z  INFO perps_market_params: Computed DNF sensitivity: 137453.49296446526
```
