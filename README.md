# Introduction

This is perps v2, following a completely new financial model, without a vAMM

It's also an updated dev environment, with multichain support and a native SDK in both Typescript and Rust/WASM (no Rust binary sdk yet..)

- Long-form documentation
    - [Whitepaper](https://www.notion.so/levana-protocol/Levana-Well-funded-Perpetuals-Whitepaper-9805a6eba56d429b839f5551dbb65c40)
    - [High level overview](https://docs.levana.exchange/high-level-overview)
    - [Slides describing the platform](https://docs.levana.exchange/slides/) (primarily intended for audit)
    - [API tutorial in TypeScript](https://docs.levana.exchange/api-tutorial-ts/introduction)
- References docs
    - [levana_perpswap_cosmos](https://apidocs.levana.finance/levana_perpswap_cosmos/)
- [Web interfaces](https://staff.levana.finance/perps-sites)

# PREREQUISITES

1. [Rust](https://www.rust-lang.org/tools/install)
2. Docker, used by default for building and optimizing the smart contracts
3. [just](https://github.com/casey/just), optional, but a recommended way to perform common tasks
4. (optional, for typescript) [Node](https://nodejs.org/en/download/)

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

## Proptests and Fuzz testing

* `cargo test --features proptest` runs prop tests
* `cargo install cargo-fuzz` to install the fuzz testing tool
* `just fuzz`

# Getting started with various chains

## Faucets

* Juno: https://docs.junonetwork.io/validators/joining-the-testnets#get-some-testnet-tokens
* Osmosis: https://faucet.osmosis.zone/#/

# Deploying

Deploying is handled via the `perps-deploy` tool, located in the
`packages/perps-deploy` directory. The [perps-deploy.md](./docs/perps-deploy.md) includes
more details of how deployments work, this file covers the direct
steps.

When you deploy, you'll need to have the deployer seed phrase. This is available in a [Google Drive sheet](https://docs.google.com/spreadsheets/d/1ILEkU8wqtQGO_bqxsSVORflwtY-4kj20dmTe9uOh3-4/edit?usp=share_link). You'll also need to choose which contract family you want to deploy, e.g. `dragonci`, `dragondev`, `dragonqa`. Let's assume you'll be deploying `dragonci`.

1. Build the WASM contracts: `just build-contracts`
    * Or, with native tools: `just build-contracts-native`
2. Set your seed phrase to an environment variable: `export COSMOS_WALLET="deployer seed phrase"`
3. Set the appropriate contract family: `export PERPS_FAMILY=dragonci`
4. Store the WASM code on the blockchain: `cargo run --bin perps-deploy testnet store-code`
5. To deploy a fresh set of contracts: `cargo run --bin perps-deploy testnet instantiate`
6. To migrate an existing set of contracts: `cargo run --bin perps-deploy testnet migrate`

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
