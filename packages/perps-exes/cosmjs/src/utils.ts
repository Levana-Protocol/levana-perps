import * as path from "path";
import * as fs from "fs/promises"; 
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate"
import { 
    Account,
    calculateFee ,
    coins,
    DeliverTxResponse,
    GasPrice,
    Event,
    QueryClient,
    createProtobufRpcClient,
    ProtobufRpcClient,
    StdFee
} from "@cosmjs/stargate"
import { Coin, DirectSecp256k1HdWallet, Registry } from "@cosmjs/proto-signing"
import { ENV_SEED_PHRASE, NETWORKS } from "./config";
import { QueryClientImpl } from "cosmjs-types/cosmwasm/wasm/v1/query";
import { fromUtf8, toUtf8 } from "@cosmjs/encoding";



export interface Wallet {
    signer: DirectSecp256k1HdWallet,
    client: SigningCosmWasmClient,
    address: string,
    account: Account,
    rpcClient: ProtobufRpcClient,
    queryService: QueryClientImpl,
}

type RegistryUpdater = (registry:Registry) => void;
export async function getWallet(config, registryUpdater?:RegistryUpdater):Promise<Wallet> {
    const {
        target,
        addr_prefix,
        rpc_url,
        denom,
        gasPrice
    } = config;

    const seed_phrase = process.env[ENV_SEED_PHRASE];
    if (!seed_phrase || seed_phrase === "") {
        throw new Error(`Please set ${ENV_SEED_PHRASE} in .env`);
    }

    const signer = await DirectSecp256k1HdWallet.fromMnemonic(
        seed_phrase, 
        { 
            prefix: addr_prefix,
        }
    );


    const accounts = await signer.getAccounts()
    const address = accounts[0].address

    const client = await SigningCosmWasmClient.connectWithSigner(
        rpc_url,
        signer,
        { gasPrice }
    );

    if(registryUpdater) {
        registryUpdater(client.registry);
    }

    const account = await client.getAccount(address);
    if(!account) {
        throw new Error(`Account ${address} does not exist`);
    }
    // const balance = await client.getBalance(address, denom);

    // console.log(`Wallet address is ${address}`)
    // console.log(`Account information: ${JSON.stringify(account)}`)
    // console.log(`Balance is ${balance.amount}${balance.denom}`)

    const queryClient:QueryClient = (client as any).forceGetQueryClient();
    const rpcClient:ProtobufRpcClient = createProtobufRpcClient(queryClient);
    const queryService = new QueryClientImpl(rpcClient);

    return { 
        client, 
        address, 
        signer, 
        account, 
        rpcClient,
        queryService,
    };
}

export async function instantiateContract(wallet, codeId, contract_name, instantiate_msg) {

    const instantiateReceipt = await wallet.client.instantiate(
        wallet.address,
        codeId,
        instantiate_msg,
        contract_name,
        "auto",
        {
            admin: wallet.address
        }
    )

    const { contractAddress } = instantiateReceipt
    if(!contractAddress || contractAddress === "") {
        throw new Error("Failed to instantiate contract");
    }

    console.log("instantiated", contract_name, "at", contractAddress);

    return contractAddress;
}

export async function uploadContract(wallet, contract_path) {
    const contents = await fs.readFile(contract_path);
    const uploadReceipt = await wallet.client.upload(wallet.address, contents, "auto");
    const {codeId} = uploadReceipt;

    if(!codeId || codeId === "") {
        throw new Error("Failed to upload contract");
    }

    console.log(`Contract uploaded with code ID ${codeId}`);

    return codeId;
}

export async function queryContract(wallet, contractAddress, msg) {
    const queryContractSimple = async () => {
        return await wallet.client.queryContractSmart(contractAddress, msg);
    }

    // the hard manual way with protobuf definitions 
    const queryContractManual = async () => {
        const request = { address: contractAddress, queryData: toUtf8(JSON.stringify(msg)) };
        const resp = await wallet.queryService.SmartContractState(request);

        // By convention, smart queries must return a valid JSON document (see https://github.com/CosmWasm/cosmwasm/issues/144)
        let responseText: string;
        try {
            responseText = fromUtf8(resp.data);
        } catch (error) {
            throw new Error(`Could not UTF-8 decode smart query response from contract: ${error}`);
        }
        try {
            return JSON.parse(responseText);
        } catch (error) {
            throw new Error(`Could not JSON parse smart query response from contract: ${error}`);
        }
    }

    return await queryContractSimple();
}

export async function execContract(wallet, contractAddress, msg, fee: StdFee | "auto" | number, memo = "", funds?: readonly Coin[]) {
    return await wallet.client.execute(contractAddress, msg, fee, memo, funds);
}

export async function getNetworkConfig() {
    const target = await getTarget();
    const config = NETWORKS[target];
    return { 
        target,  
        gasPrice: GasPrice.fromString(config.gas_price + config.denom),
        ...config
    };
}

export async function getTarget() {
    const target = getArg("target");

    const availableTargets = Object.keys(NETWORKS);

    if (availableTargets.includes(target)) {
        return target;
    }

    throw new Error(`Please specify a target with --target=[${availableTargets.join(" | ")}]`);

}

export function getArg(key:string):string {
    for (const arg of process.argv) {
        if (arg.startsWith(`--${key}=`)) {
            const value = arg.substring(key.length + 3);
            if (value && value !== "") {
                return value;
            }
        }
    }

    throw new Error(`Please specify a value for --${key}`);
}

export async function queryContractInfo(wallet, address) {
    return await wallet.client.getContract(address);
}

export function firstEvent(resp:DeliverTxResponse, type:string) {
    return resp.events.find(e => e.type === type)
}

export function firstAttribute(event: Event, key: string) {
    return event.attributes.find(e => e.key == key)?.value;
}
