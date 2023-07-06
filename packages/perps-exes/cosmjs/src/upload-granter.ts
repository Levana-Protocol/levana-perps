import { getArg, getNetworkConfig, getWallet, queryContractInfo, uploadContractGranter,} from "./utils";
import * as fs from "fs/promises"; 
import * as path from "path";
import { WASM_ARTIFACTS_PATH } from "./config";

(async () => {
    const networkConfig = await getNetworkConfig();
    const wallet = await getWallet(networkConfig);
    const contract = getArg("contract");

    await uploadContractGranter(wallet, path.join(WASM_ARTIFACTS_PATH, `levana_perpswap_cosmos_${contract}.wasm`), "osmo1lqyn9ncwkcqj8e0pnugu72tyyfehe2tre98c5qfzjg4d3vdw7n5q5a0x37");
})();
