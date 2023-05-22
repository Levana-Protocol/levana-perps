import { execContract, getArg, getNetworkConfig, getWallet, queryContractInfo, uploadContract,} from "./utils";
import * as fs from "fs/promises"; 
import * as path from "path";
import { WASM_ARTIFACTS_PATH } from "./config";

(async () => {
    const networkConfig = await getNetworkConfig();
    const wallet = await getWallet(networkConfig);

    while (true) {
    const res = await wallet.client.execute(wallet.address, "sei13utpdt0xflyvh4zgqrfjlhvgvhnacj5rme2lgc89p9t9k0qsf4jqekkekc", {crank:{}},"auto")

    console.log(JSON.stringify(res))
    }
})();
