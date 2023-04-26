# perps-deploy

Command line tool providing functionality for working with perps, providing deployment and on-chain testing.

## Basic workflow

* One time per chain
    * Deploy a new copy of the `tracker` contract to the chain
    * Update `assets/config.yaml` to include that address
* Each time there is a new version of a contract that you'd like to upload
    * Build the contracts (can use `../.ci/contracts.sh`)
    * Use the `store-code` subcommand to upload the contracts
    * Will log information with the tracker contract about the contracts uploaded
    * Will automatically skip uploading if any contract is already present (based on its SHA256 hash)
* To instantiate a fresh set of contracts
    * Use the `instantiate` subcommand
    * Provide the `--family` flag to indicate the family of contracts (e.g., `dragondev`)
    * Will instantiate fresh copies of `faucet`, `cw20`, and `factory`
    * Adds a new market (which causes a new market, position token, and both LP and xLP liquidity tokens)
    * Sets the price admin on the factory contract to the price admin bot for the contract family specified
    * (Both above points leverage `assets/config.yaml`)
    * Logs all the new contracts in the `tracker`
* To migrate an existing set of contracts
    * Use the `migrate` subcommand
    * Provide the `--family` flag
    * Looks up latest deployment for the given family
    * Updates factory contract with new code IDs for market, liquidity token, and position token
    * Migrates the factory contract
    * Migrates the market, liquidity token, and position token contracts associated with the factory

## Unsupported (for now)

* Does not run on-chain tests (they're still written in TypeScript)

This version is exclusively about getting automated migrations onto CI and
letting external tooling (frontend, bots, indexer, and QA CLI) discover the
contracts.

## Adding new contract families

The `assets/config.yaml` file contains contains information for individual tracker contracts per chain as well as families of contracts. For more information on contract families, see [the Notion documentation](https://www.notion.so/levana-protocol/Perps-Environments-23a8906c16004c52b1b8ccfc09392ed3). If you would like to create a new contract family, follow these steps:

* Determine a name for the family, e.g. `osmoqa`
* Generate a new set of seed phrases for the bots which will run on these contracts
    * You can use the `cosmos` CLI to generate these seed phrases, e.g. `cosmos gen-wallet osmo` will generate a new seed phrase and print the Osmosis address
    * You will need three new seed phrases: the price admin (aka oracle), bot wallet manager, and crank bot
    * You will need the public addresses of the first two for updating the config here
*   Within `assets/config.yaml`, add a stanza like the following:

    ```yaml
    families:
      osmoqa:
        network: osmosis-testnet
        wallet-manager-address: osmo1.... # from the gen-wallet above
        price: osmo1... # from the gen-wallet above, but for price admin
        # faucet and cw20 are a bit complicated right now
        # For non-trading-competition, you can copy-paste one of the values
        # from another family on the same network, or you can leave it blank.
        # This should get sorted better in the future.
    ```

* Add the three new seed phrases to the `amber.yaml` file within the `perps-bots` repo. You can use the `amber encrypt` command to do so. Follow the naming scheme of the other seed phrases in that file. See [amber](https://github.com/fpco/amber) if you're unfamiliar with the tool.
