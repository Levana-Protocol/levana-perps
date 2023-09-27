import * as dotenv from "dotenv";
import * as path from "path";

dotenv.config({ path: path.resolve('../../../.env') });

export const ENV_SEED_PHRASE = "COSMOS_WALLET";
export const WASM_ARTIFACTS_PATH = "../../../wasm/artifacts";

export const NETWORKS = {
    "juno-testnet": {
      "rpc_url": "https://rpc.uni.junonetwork.io",
      "rest_url": "https://api.uni.junonetwork.io",
      "gas_price": "0.025",
      "full_denom": "JUNOX",
      "denom": "ujunox",
      "chain_id": "uni-6",
      "addr_prefix": "juno"
    },
    "osmosis-testnet": {
      "rpc_url": "https://rpc.osmotest5.osmosis.zone",
      "rest_url": "https://lcd.osmotest5.osmosis.zone",
      "gas_price": "0.025",
      "full_denom": "OSMO",
      "denom": "uosmo",
      "chain_id": "osmo-test-5",
      "addr_prefix": "osmo"
    },
    "osmosis-mainnet": {
      "rpc_url": "https://rpc.dev-osmosis.zone",
      "rest_url": "https://lcd.osmotest5.osmosis.zone",
      "gas_price": "0.025",
      "full_denom": "OSMO",
      "denom": "uosmo",
      "chain_id": "osmosis-1",
      "addr_prefix": "osmo"
    },
    "juno-mainnet": {
      "rpc_url": "https://juno-rpc.polkachu.com",
      "rest_url": "https://juno-api.polkachu.com",
      "gas_price": "0.025",
      "full_denom": "JUNO",
      "denom": "ujuno",
      "chain_id": "juno-1",
      "addr_prefix": "juno"
    },
    "stargaze-testnet": {
      "rpc_url": "https://rpc.elgafar-1.stargaze-apis.com",
      "rest_url": "https://rest.elgafar-1.stargaze-apis.com",
      "gas_price": "0.025",
      "full_denom": "STARS",
      "denom": "ustars",
      "chain_id": "elgafar-1",
      "addr_prefix": "stars"
    },
    "sei-testnet": {
      "rpc_url": "https://test-sei.kingnodes.com",
      "rest_url": "https://sei-testnet-2-rest.brocha.in",
      "gas_price": "0.1",
      "full_denom": "SEI",
      "denom": "usei",
      "chain_id": "atlantic-2",
      "addr_prefix": "sei"
    },
    "sei-mainnet": {
      "rpc_url": "https://rpc.wallet.pacific-1.sei.io",
      "rest_url": "https://rest.wallet.pacific-1.sei.io",
      "gas_price": "0.1",
      "full_denom": "SEI",
      "denom": "usei",
      "chain_id": "pacific-1",
      "addr_prefix": "sei"
    }
}