# Introduction

This is perps v2, following a completely new financial model, without a vAMM

It's also an updated dev environment, with multichain support and a native SDK in both Typescript and Rust/WASM (no Rust binary sdk yet..)

- Long-form documentation
    - [Whitepaper](https://www.notion.so/levana-protocol/Levana-Well-funded-Perpetuals-Whitepaper-9805a6eba56d429b839f5551dbb65c40)
    - [High level overview](https://docs.levana.exchange/high-level-overview)
    - [Slides describing the platform](https://docs.levana.exchange/slides/) (primarily intended for audit)
    - [API tutorial in TypeScript](https://docs.levana.exchange/api-tutorial-ts/introduction)
- References docs
    - [`msg` crate](https://apidocs.levana.exchange/msg/doc/levana_perpswap_cosmos_msg/)
    - [`shared` crate](https://apidocs.levana.exchange/shared/doc/levana_perpswap_cosmos_shared/)
- Web interfaces
    - [Develop frontend](https://levana-web-app-develop.vercel.app/)
    - [Staging frontend](https://levana-web-app-staging.vercel.app/)
    - [Public beta](https://beta.levana.exchange/)

# PREREQUISITES

1. [Rust](https://www.rust-lang.org/tools/install)
2. Docker, used by default for building and optimizing the smart contracts
3. [just](https://github.com/casey/just), optional, but a recommended way to perform common tasks
4. (optional, for typescript) [Node](https://nodejs.org/en/download/)
5. (optional, for manual building) [wasm-opt](https://github.com/WebAssembly/binaryen/releases) (extract anywhere and edit .env)

# Testing

* `cargo test` runs the minimal off-chain tests
* `just cargo-test-check` runs off-chain tests in more configurations and checks the codebase
* `just build-contracts` will build the WASM files
  * alternatively, `just build-contracts-native` to bypass Docker and build with native tooling

## On-chain w/ LocalOsmosis

* `just run-localosmo` launches a local Osmosis instance
* `COSMOS_WALLET` env var should be set to the correct seed phrase
* `just local-deploy` deploys a copy of the contracts to your Local Osmosis
* `just contracts-test` will launch Local Osmosis, deploy contracts to it, and then run on-chain tests

## On-chain w/ wasmd

* [Spin up an instance of wasmd](#basic-wasmd-setup)
* `COSMOS_WALLET` env var should be set to the correct seed phrase
* `just local-deploy-wasmd` deploys a copy of the contracts to your wasmd instance
* `just contracts-test-wasmd` will test those deployed contracts on wasmd

## Proptests and Fuzz testing

* `cargo test --features proptest` runs prop tests
* `cargo install cargo-fuzz` to install the fuzz testing tool
* `just fuzz`

## Diagnostics GUI

* [install trunk.rs](https://trunkrs.dev/#install) - install the Trunk builder for frontend rust/wasm
* `yarn install` in `packages/diagnostics` - install the yarn dependencies
* `just diagnostics-gui [dev/release] [sanity/performance]` - run things

  where `sanity` means to run with sanity checks, `performance` means without, `dev` means to run debug builds, and `release` means to run production builds

  so for example, `just diagnostics-gui dev sanity` will capture all the debug info, but be slow, while `just diagnostics-gui release performance` will miss some debug info and assertions, but run fast

  a log of all actions is written to the .gitignored `bridge.log`


# Getting started with various chains

## Faucets

* Juno: https://docs.junonetwork.io/validators/joining-the-testnets#get-some-testnet-tokens
* Osmosis: https://faucet.osmosis.zone/#/

# MISC

the [original repo](https://github.com/Levana-Protocol/levana-perpetual-swap-contracts) is deprecated, though a sortof middle-ground partial refactor of the old vAMM approach is in [this commit](https://github.com/Levana-Protocol/levana-perps-multichain/tree/2417dc2e0ba3030ab8ed3cd0f1fc6f2ddaf39843/contracts/cosmos)

# Deploying

Deploying is handled via the `perps-deploy` tool, located in the `packages/perps-deploy` directory. The [README.md](packages/perps-deploy/README.md) includes more details of how deployments work, this file covers the direct steps.

When you deploy, you'll need to have the deployer seed phrase. This is available in a [Google Drive sheet](https://docs.google.com/spreadsheets/d/1ILEkU8wqtQGO_bqxsSVORflwtY-4kj20dmTe9uOh3-4/edit?usp=share_link). You'll also need to choose which contract family you want to deploy, e.g. `dragonci`, `dragondev`, `dragonqa`. Let's assume you'll be deploying `dragonci`.

1. Build the WASM contracts: `just build-contracts`
    * Or, with native tools: `just build-contracts-native`
2. Set your seed phrase to an environment variable: `export COSMOS_WALLET="deployer seed phrase"`
3. Set the appropriate contract family: `export PERPS_FAMILY=dragonci`
4. Store the WASM code on the blockchain: `cargo run --bin perps-deploy testnet store-code`
5. To deploy a fresh set of contracts: `cargo run --bin perps-deploy testnet instantiate`
6. To migrate an existing set of contracts: `cargo run --bin perps-deploy testnet migrate`

# Basic wasmd setup

Not a requirement, but if you are targetting vanilla wasmd it assumes certain configuration setup, and as of right now there isn't good general information about getting it up and running

First, build and install: `make install` (or on apple silicon: `LEDGER_ENABLED=false make install`)

Next, configuration...

our chain-id is going to be `localwasmd`
our gas denomination is going to be configured to `uwasm`
staking denomination is the default `ustake`

the following sortof mimics the explicit steps in https://github.com/CosmWasm/wasmd/blob/main/contrib/local/setup_wasmd.sh

adding two users: tester1 and validator1

* wipe `~/.wasmd` if it exists
* `wasmd init localwasmd --chain-id localwasmd --overwrite`
* edit `~/.wasmd/config/app.toml`
  * set `minimum-gas-prices` to `0.025uwasm`
* edit `~/.wasmd/config/client.toml`
  * set `chain-id` to `localwasmd`
* edit `~/.wasmd/config/genesis.json`
  * change `stake` to `ustake`
* edit `~/.wasmd/config/config.toml`
  * under Consensus Configuration: change all the `timeout_` stuff to `200ms`
* `wasmd keys add tester1`
* wasmd add-genesis-account $(wasmd keys show -a tester1) 10000000000uwasm,10000000000ustake
* `wasmd keys add validator1`
* wasmd add-genesis-account $(wasmd keys show -a validator1) 10000000000uwasm,10000000000ustake
* wasmd gentx validator1 "250000000ustake" --chain-id="localwasmd" --amount="250000000ustake"
* wasmd collect-gentxs

now `wasmd start` should just work

# Coverage

## Installation

- Install [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov/releases)
- Install llvm-tools-preview component from rustup:

``` shellsession
❯ rustup component add llvm-tools-preview
```

## Usage

Build coverage report by running off chain tests under different
configurations:

``` shellsession
❯ just off-chain-coverage
...
```

Then based on the kind of output you want, run these recipies:

- For an HTML based report:

``` shellsession
❯ just off-chain-html-coverage
```

- For a summary on the terminal:

``` shellsession
❯ just off-chain-term-coverage
```
