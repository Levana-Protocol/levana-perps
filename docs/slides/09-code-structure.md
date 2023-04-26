---
title: Code structure
---
# Code structure

* Monorepo for contracts, messages, tests, and support tools
* Contains prod and testnet-only contracts
* Contracts themselves live in `contracts` directory (compat with Rust workspace optimizer)
    * Relevant mainnet subdirectories: `factory`, `liquidity_token`, `market`, `position_token`
* `packages` contains
    * `shared`: helper types and functions for contracts and tooling
    * `msg`: messages for the contract API
    * `perps-exes`: tooling (deployment, bots, on-chain tests)
    * `multi_test`: off-chain tests and proptests
    * `fuzz`: fuzz testing (not highly used)
    * `diagnostics`: web UI for simulating random activities
* `research`: tooling for simulating protocol interactions
* `ts-schema`: ability to generate TypeScript types from `msg` crate
---
---
# Code structure comments

## Data types

Codebase uses a `PricePoint` data point to collect all relevant price info for a timestamp and provide easy conversions.

### Numerical data types

### State and StateContext

### Events and ResponseBuilder

### Internal/external, base/notional

## Error handling
