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
    ProtobufRpcClient
} from "@cosmjs/stargate"
import { DirectSecp256k1HdWallet, Registry } from "@cosmjs/proto-signing"
import { ENV_SEED_PHRASE, NETWORKS } from "./config";



export interface Wallet {
    signer: DirectSecp256k1HdWallet,
    client: SigningCosmWasmClient,
    address: string,
    account: Account,
    rpcClient: ProtobufRpcClient
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

    return { 
        client, 
        address, 
        signer, 
        account, 
        rpcClient,
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
    return await wallet.client.queryContractSmart(contractAddress, msg);
}

export async function execContract(wallet, contractAddress, msg) {
    return await wallet.client.execContract(contractAddress, msg);
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
