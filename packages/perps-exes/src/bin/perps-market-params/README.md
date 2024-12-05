# perps-market-params

This tool aids in checking of market parameters. Currently it only
supports DNF sensitivity check.
# Prepare your test environment
You need to install aws-cli in your system from https://aws.amazon.com/cli/
```bash
# Check your version
aws --version
```
Configure your profile in a file. Example `aws-config`
```bash
[profile lvn-sandbox]
sso_region = ap-northeast-2
sso_start_url = https://levanafinance.awsapps.com/start
sso_account_id = 264635650474
sso_role_name = AdministratorAccess
region = eu-west-3
```
Set these environment variables
```bash
export AWS_CONFIG_FILE="/home/norys/fpco/levana/aws/aws-config"
export AWS_PROFILE="lvn-sandbox"
```
Sign in to AWS
```bash
aws sso login --profile lvn-sandbox
```
Set these environment variables
```bash
# S3 test bucket
export LEVANA_MPARAM_S3_BUCKET_ID=levtest-frontend-cache
# Ask for this
export LEVANA_MPARAM_SLACK_WEBHOOK=https://hooks.slack.com/services/xxxxxxxxxxxxxxxxxxxxxxx
```
Now you can test perps_market_params
```bash
# Ask for cmc-key
cargo run --bin perps-market-params -- --cmc-key=xxxxxxxxxxxxxxxxxxxxxxxxxxx serve
```
# Usage

``` shellsession
Usage: perps-market-params [OPTIONS] --cmc-key <CMC_KEY> <COMMAND>

Commands:
  coins      List supported coins with their IDs
  markets    List all market ids of Levana
  exchanges  List all exchanges for a specific market id
  dnf        Compute DNF sensitivity
  market     Download market data in csv
  serve      Serve web application
  help       Print this message or the help of the given subcommand(s)

Options:
      --verbose            Verbose flag
      --cmc-key <CMC_KEY>  CMC key [env: LEVANA_MPARAM_CMC_KEY=REDACTED]
  -h, --help               Print help
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

To see DNF computation for the OSMO_USD market:

```
❯ cargo run --bin perps-market-params -- dnf --market-id OSMO_USD
    Finished dev [unoptimized + debuginfo] target(s) in 0.44s
     Running `/home/sibi/fpco/github/levana/levana-perps/target/debug/perps-market-params dnf --market-id OSMO_USD`
2024-04-05T03:58:42.211871Z  INFO perps_market_params: Computed DNF sensitivity: 27513616.890668117
```

# Adding new markets

Before adding a new market, you would need to ensure that we have all
the Exchanges types (CEX or DEX) mapped in our code.

Let's say you are going to add a new market id `PYTH_USDC`, you would
need to do this to see if there are any unspported exchanges:

``` shellsession
cargo run --bin perps-market-params exchanges --market-id PYTH_USDC
    Finished dev [unoptimized + debuginfo] target(s) in 0.43s
     Running `/home/sibi/fpco/github/levana/levana-perps/target/debug/perps-market-params exchanges --market-id PYTH_USDC`
2024-04-05T04:00:57.599209Z  INFO perps_market_params: Unsupported exchange: Raydium (slug: raydium, id: 1342)
2024-04-05T04:00:57.599306Z  INFO perps_market_params: Unsupported exchange: Orca (slug: orca, id: 1426)
2024-04-05T04:00:57.599331Z  INFO perps_market_params: Unsupported exchange: Jupiter (slug: jupiter, id: 1612)
2024-04-05T04:00:57.599351Z  INFO perps_market_params: Unsupported exchange: Salavi Exchange (slug: salavi-exchange, id: 8388)
2024-04-05T04:00:57.599429Z  INFO perps_market_params: Unsupported exchange: BiKing (slug: biking, id: 9298)
2024-04-05T04:00:57.599452Z  INFO perps_market_params: Unsupported exchange: Backpack Exchange (slug: backpack-exchange, id: 9452)
```

Now you need to make sure to map those exchange ids with it's type
properly. Unfortunately CoinMarketCap doesn't provide an API to do
this automatically as of now (even though they seem to have the data
internally), so we manually map it.
