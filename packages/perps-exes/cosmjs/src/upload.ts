import { getArg, getNetworkConfig, getWallet, queryContractInfo, uploadContract,} from "./utils";
import * as fs from "fs/promises"; 
import * as path from "path";
import { WASM_ARTIFACTS_PATH } from "./config";

(async () => {
    const networkConfig = await getNetworkConfig();
    const wallet = await getWallet(networkConfig);
    const contract = getArg("contract");

    await uploadContract(wallet, path.join(WASM_ARTIFACTS_PATH, `levana_perpswap_cosmos_${contract}.wasm`));
})();