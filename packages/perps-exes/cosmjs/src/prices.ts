import { getNetworkConfig, getWallet, queryContract, monkeyPatchQueryClient} from "./utils";
import * as fs from "fs/promises"; 
import * as path from "path";
import { WASM_ARTIFACTS_PATH } from "./config";

const MARKET_ADDR = "sei13utpdt0xflyvh4zgqrfjlhvgvhnacj5rme2lgc89p9t9k0qsf4jqekkekc";
const SLEEP_MS = 100;

interface SpotPriceResponse {
    price_base:string,
    timestamp:string
}

(async () => {
    const networkConfig = await getNetworkConfig();
    const wallet = await getWallet(networkConfig);

    monkeyPatchQueryClient(wallet, resp => console.log(`query height: ${resp.height}`));

    while (true) {
        const res:SpotPriceResponse = await queryContract(wallet, MARKET_ADDR, {spot_price: {}});
        const timestamp = parseInt(res.timestamp); 
        const price_base = parseFloat(res.price_base);

        console.log({price_base, timestamp});

        await new Promise((resolve) => setTimeout(resolve, SLEEP_MS));
    }
})();
