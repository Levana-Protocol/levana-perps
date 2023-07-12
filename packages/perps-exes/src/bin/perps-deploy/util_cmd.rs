use std::path::PathBuf;

use anyhow::{Context, Result};
use cosmos::{Address, CosmosNetwork, HasAddress, TxBuilder};
use perps_exes::{
    config::{ChainConfig, PythConfig},
    pyth::{get_oracle_update_msg, VecWithCurr},
};
use serde_json::json;
use shared::storage::MarketId;

#[derive(clap::Parser)]
pub(crate) struct UtilOpt {
    #[clap(subcommand)]
    sub: Sub,
}

#[derive(clap::Parser)]
enum Sub {
    /// Set the price in a Pyth oracle
    UpdatePyth {
        #[clap(flatten)]
        inner: UpdatePythOpt,
    },
    /// Deploy a new Pyth contract
    DeployPyth {
        #[clap(flatten)]
        inner: DeployPythOpt,
    },
}

impl UtilOpt {
    pub(crate) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        match self.sub {
            Sub::UpdatePyth { inner } => update_pyth(opt, inner).await,
            Sub::DeployPyth { inner } => deploy_pyth_opt(opt, inner).await,
        }
    }
}

#[derive(clap::Parser)]
struct UpdatePythOpt {
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: CosmosNetwork,
    /// Market ID to do the update for
    #[clap(long)]
    market: MarketId,
    /// Override the oracle used
    #[clap(long)]
    oracle: Option<Address>,
    /// Override Pyth config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_PYTH")]
    pub(crate) config_pyth: Option<PathBuf>,
    /// Override chain config file
    #[clap(long, env = "LEVANA_BOTS_CONFIG_CHAIN")]
    pub(crate) config_chain: Option<PathBuf>,
}

async fn update_pyth(
    opt: crate::cli::Opt,
    UpdatePythOpt {
        market,
        network,
        oracle,
        config_pyth,
        config_chain,
    }: UpdatePythOpt,
) -> Result<()> {
    let basic = opt.load_basic_app(network).await?;
    let pyth = PythConfig::load(config_pyth)?;
    let endpoints = VecWithCurr::new(pyth.endpoints.clone());
    let client = reqwest::Client::new();
    let feeds = pyth
        .markets
        .get(&market)
        .with_context(|| format!("No Pyth feed data found for {market}"))?;

    let oracle = match oracle {
        Some(oracle) => oracle,
        None => {
            let chain = ChainConfig::load(config_chain, network)?;
            chain
                .pyth
                .with_context(|| format!("No Pyth oracle found for network {network}"))?
        }
    };
    let oracle = basic.cosmos.make_contract(oracle);

    let msg = get_oracle_update_msg(feeds, &basic.wallet, &endpoints, &client, &oracle).await?;

    let builder = TxBuilder::default().add_message(msg);
    let res = builder
        .sign_and_broadcast(&basic.cosmos, &basic.wallet)
        .await?;
    log::info!("Price set in: {}", res.txhash);
    Ok(())
}

#[derive(clap::Parser)]
struct DeployPythOpt {
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: CosmosNetwork,
    /// File containing wormhole WASM
    #[clap(long)]
    wormhole: PathBuf,
    /// File containing Pyth oracle WASM
    #[clap(long)]
    pyth_oracle: PathBuf,
}

async fn deploy_pyth_opt(
    opt: crate::cli::Opt,
    DeployPythOpt {
        network,
        wormhole,
        pyth_oracle,
    }: DeployPythOpt,
) -> Result<()> {
    // What are these magical JSON messages below? They're taken directly from
    // the upload to Osmosis testnet. See these links:
    //
    // - https://testnet.mintscan.io/osmosis-testnet/wasm/contract/osmo12u2vqdecdte84kg6c3d40nwzjsya59hsj048n687m9q3t6wdmqgsq6zrlx
    // - https://testnet.mintscan.io/osmosis-testnet/wasm/contract/osmo1224ksv5ckfcuz2geeqfpdu2u3uf706y5fx8frtgz6egmgy0hkxxqtgad95
    // - https://testnet.mintscan.io/osmosis-testnet/txs/0C75CE16C91F32A902E43A6326B63800DA5182EFC52AA245E101C6374E3671B1?height=481108
    // - https://testnet.mintscan.io/osmosis-testnet/txs/F58EF5AC1A1941362339A2355F2A2DD44BF46522C37E3D60602C0E731B36F0B6?height=481109
    // - https://testnet.mintscan.io/osmosis-testnet/txs/59984BB3216E6A7D44501B11EE1F51735E9DE9C8D24D87343B9DDB480F3B5ED3?height=481110
    let basic = opt.load_basic_app(network).await?;

    let wormhole = basic
        .cosmos
        .store_code_path(&basic.wallet, &wormhole)
        .await?;
    log::info!("Uploaded wormhole contract: {wormhole}");

    let pyth_oracle = basic
        .cosmos
        .store_code_path(&basic.wallet, &pyth_oracle)
        .await?;
    log::info!("Uploaded Pyth oracle contract: {pyth_oracle}");

    let gas_denom = basic.cosmos.get_gas_coin();

    let wormhole_init_msg = json!({
        "chain_id": 60014,
        "fee_denom": gas_denom,
        "gov_chain": 1,
        "gov_address": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQ=",
        "guardian_set_expirity": 86400,
        "initial_guardian_set": {
            "addresses": [
            {
                "bytes": "WMw65cCXshPOPIGXnhuflXB0aqU="
            }
            ],
            "expiration_time": 0
        }
    });
    let wormhole = wormhole
        .instantiate(
            &basic.wallet,
            "Test Wormhole Contract",
            vec![],
            wormhole_init_msg,
            cosmos::ContractAdmin::Sender,
        )
        .await?;
    log::info!("Deployed new wormhole contract: {wormhole}");

    let mut builder = TxBuilder::default();
    builder.add_execute_message_mut(&wormhole, &basic.wallet, vec![], json!({
        "submit_v_a_a": {
            "vaa": "AQAAAAABAHrDGygsKu7rN/M4XuDeX45CHTC55a6Lo9Q3XBx3qG53FZu2l9nEVtb4wC0iqUsSebZbDWqZV+fThXQjhFrHWOMAYQrB0gAAAAMAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAABTkAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAENvcmUCAAAAAAABE1jMOuXAl7ITzjyBl54bn5VwdGql/2y5Ulib3oYsJe9DkhMvudSkIVcRTehGAZO986L8+B+GoJdl9HYv0RB6AIazLXoJd5JqIFEx2HMdOcvrjIKy/YL67ScR1Zrw8kmdFucm9rIRs5dWwEJEG+bYZQtptU6+cV4jQ1TOW000j7dLlY6JZuLsPb1JWKfN619zifomlBUZ8IYzScIjtzpt3ud0o7+ROVPWlSYNiLwaolpO7jY+8AAKwAdnJ7NfvqLawo/uXMsP6naOr0XO0Ta52eJJA0ZK6In1yKcj/BT5MSS3xziEPLuJ6GTIYsOM3czPldLMN6TcA2qNIytI9izdRzFBL0iQ2nmPaJajMx9ktIwS0dV/2cvnCBFxqhvh02yv44Z5EPmcCeNHiZwZw4GStuc4fM12gnfBfasbelAnwLPPF44hrS53rgZxFUnPux+cep2AluheFIfzVRXQKpJ1NQSo11RxufSe22++vImPQD5Hc+lf6xXoDJqZyDSN"
        }
    }))?;
    builder.add_execute_message_mut(&wormhole, &basic.wallet, vec![], json!({
        "submit_v_a_a": {
            "vaa": "AQAAAAENABLms5xtqQxd/Twijtu3jHpMl8SI/4o0bRYakdsGflHWOMFyFvNoqpvfSDa4ZFqYAYymfS/sh9dpyr/fJAa/eQoAAu9CsogJGmcO81VllvT0cyNxeIKIHq844DNFB40HoVbzEreFtk2ubpqH49MocvWcsZMfcozs9RF2KYG69IMDZo8BA87yYWuExOUR/wMynghT8b1+6axbpx1wpNdhCL3flPacKoqE5O6UBl6AA8M06JkYSUNjThIEPQ3aeNk5ltoHPRkBBOdtFmudrJj2AhB8xLRKyCho+vALY999JPF3qjkeBQkCQTtxBGQ05nx3Cxmuzff84dFDXqC+cmLj5MGPUN3IF1wBBdlFDoIW10HgIGpQ+Tt1Ckfgoli4Drj+0TFMwwCz2QUJLeJc0202YJe3EDri0YQSEym6OqLXxsxTJz8RrxR5gRABBodHfI3uyJ02oj55SP6wdN+VNi/I3L2K6RCsVWod7h51XFa5211xDJQJOO15vBiVo2RlI6WLxV9HWiNDWjc+z90BB/sGc0hk953vThkklzYlExcVMNrqgfB/u59piv5+ZsbUTbITIxRPJlfUpThqlUu5Tu+fZBSMM6725Hfq+ixcmEwBCIdp6CIWMQ0YJ9m9SGRewj6Q3k74qN6Z4tNR0d8xhghWYkjYDNyDvcrDgrPDDGcDUr6H+Qaaq1A30LdHII6unGUBCel5ZJf/kQbQ0cYuGE2DcWKChwzvYaHuE9b8SFtSGtzOJVyW99G8qNjn59RUtleDqDC93J2UCSCRomjTEezYTCYBDEaMn7bUECaEH/n41zaPownU2+o+pLvS/sz5SpLMiiCiJjOKjiEmzRb3Dq8VtPyb4sP6Gd7xTgcZVqYF6dGsQWIBDiP8tr1EW3wlr7ciJQway8Bh7ZZLqd4TJmCa4BKs37lpQrKhAqLemauWMnhZo0orSadn29ti4KH7Jq9g/kT9SWoAEGuwusd6xos0dkXy+xrXieqb12+5sjJPJa4G+X5lJG8ULfcX9mLnOUgxcYLGLOh9ecc97w26EuUkLfwDg4KBLP4AEm2gPF5WyxWu7OrcHhekV1OrTcDse/anXKAxQ+1KKU9vYbw/R4pFeDPkMITs18mFvy9VpV8WiqwOAw/EnoReSXEBYm6dml2eND8AAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEwXWRZ8Q/UBwgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAENvcmUCAAAAAAACE1jMOuXAl7ITzjyBl54bn5VwdGql/2y5Ulib3oYsJe9DkhMvudSkIVcRTehGAZO986L8+B+GoJdl9HYv0RB6AIazLXoJd5JqIFEx2HMdOcvrjIKy/YL67ScR1Zrw8kmdFucm9rIRs5dWwEJEG+bYZQtptU6+cV4jQ1TOW000j7dLlY6JZuLsPb1JWKfNZrlZDhxB4LImk3v5IX0dZ/1OkfV0o7+ROVPWlSYNiLwaolpO7jY+8AAKwAdnJ7NfvqLawo/uXMsP6naOr0XO0Ta52eJJA0ZK6In1yKcj/BT5MSS3xziEPLuJ6GTIYsOM3czPldLMN6TcA2qNIytI9izdRzFBL0iQ2nmPaJajMx9ktIwS0dV/2cvnCBFxqhvh02yv44Z5EPmcCeNHiZwZw4GStuc4fM12gnfBfasbelAnwLPPF44hrS53rgZxFUnPux+cep2AluheFIfzVRXQKpJ1NQSo11RxufSe22++vImPQD5Hc+lf6xXoDJqZyDSN"
        }
    }))?;
    builder.add_execute_message_mut(&wormhole, &basic.wallet, vec![], json!({
        "submit_v_a_a": {
            "vaa": "AQAAAAINAM5FR02eGx53kKLSEIceGV21OnD/1vI3z+cOJoajKFmsQ8hKMyJnqO9m9ZcZz5HMjfAQH9fDaqGHjVE5JBZg7cABA3XMkGFWrlMHhmYcDNmu9ER0e8PY1aqEysam0pM9ThoDHP+jA4PUr4Ex6SnZ8gP0YLBzCaZH1s0yqxzHckCJOSwABFIwUVbPyQNDEo+X5JkxG1yuF09Ij/IvvAlZGZGgpz2OavOvuKWWhEHTq4Q3g2QHSBc56YUK1cleas/Mhx6VG8MBBaeVbu/CPnyUWhlm1d2+nkvjdsL1TkXj1dqIwvhpJRDHQpseqGCulNkpvZfoSSOhgYfnd6o9tBmBOoDeuEzI0isABhsqTz0mZmCOCqlnN2ieO6V5OBD/OlL/KK1X2O+yCWdzXcVTei5D7xD1g9FEwSoWBlQsIH9bea8Iw4ZW06xAcTMBCGtiyOEwrzQRs8DZG1tQ3LAe1fKTlj+QH8Nuew5QEU3OIDNzsy60WXHO+CiOXZKNDtUc2G4qMAawr2plw5bACQgACek6tNLIIokBpfRSWTQACywm0dxnmgXkf98P8yMdmPvCBxAxWf9BFt8oMu6mmzgnUoNDTmzUpK8E0l+nqCmQtwcBCqZD9M9hXf/wb/1lgw9/bPZRLavDaQ1dniEP3HEoQtwnCLiywi4iTJkoDNJeXov7QOPRxVuMQXdOKHweLDUq7PwBC4nB6F+qIKMGAZZMzGp5wK5Tz9JvsQhj2zd4NCjNkTkKFjNGVYI52zzZ1CDP5COg34TIQ5l5Di4wgBG0tj5rgBUBDKMdy1ZKyBoFOiaNgJDnIJf5TzZnEdDF0TgVrx7H1H5mLi0b3iJngRPRWWPaEAtmi6JsDDJZcNBxFLg8Vpj0YJcBDcn9o5wNWS2e2SzSK1QlzGs3Qw4jbwLQ0fii70WgC94mIjwKbrNjyLJf079XI0odk2SXbO+4Ng51WiZ8u7Z0s5UBEI2wHkRKsQA92LbJb463eVi0C6eoX+/s8yrQC3pHwK51JCFiYklZd+CcCYndUPKAwhRT03VoQ2COrNF/T9/kdgAAEmECUijvWvg3ywYLzZhvz6hMzvdbP6EARoz9JOf635kWOTjzuEGjNJbCcG0CCPqrCIvRVbLiD9dMYluxzIxDZ3oBY8U8QJ4MXfoAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEbFoFTXgz0eQgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAENvcmUCAAAAAAADE1jMOuXAl7ITzjyBl54bn5VwdGql/2y5Ulib3oYsJe9DkhMvudSkIVcRTehGAZO986L8+B+GoJdl9HYv0RB6AIazLXoJd5JqIFEx2HMdOcvrjIKy/YL67ScR1Zrw8kmdFucm9rIRs5dWwEJEG+bYZQtptU6+cV4jQ1TOW000j7dLlY6JZuLsPb1JWKfNFefK8HxOPcjnxGn5LIzYj7gAWiB0o7+ROVPWlSYNiLwaolpO7jY+8AAKwAdnJ7NfvqLawo/uXMsP6naOr0XO0Ta52eJJA0ZK6In1yKcj/BT5MSS3xziEPLuJ6GTIYsOM3czPldLMN6TcA2qNIytI9izdRzFBL0iQ2nmPaJajMx9ktIwS0dV/2cvnCBFxqhvh02yv44Z5EPmcCeNHiZwZw4GStuc4fM12gnfBfasbelAnwLPPF44hrS53rgZxFUnPux+cep2AluheFIfzVRXQKpJ1NQSo11RxufSe22++vImPQD5Hc+lf6xXoDJqZyDSN"
        }
    }))?;
    let res = builder
        .sign_and_broadcast(&basic.cosmos, &basic.wallet)
        .await?;
    log::info!("VAAs set on wormhole in {}", res.txhash);

    let wormhole = wormhole.get_address_string();

    let oracle_init_msg = json!({
        "wormhole_contract": wormhole,
        "governance_source_index": 0,
        "governance_sequence_number": 0,
        "chain_id": 60014,
        "valid_time_period_secs": 60,
        "fee": {
            "amount": "1",
            "denom": gas_denom
        },
        "data_sources": [
            {
            "emitter": "a7FFCaYS8B+7xM/+69S7+0kqht9xfr6S6230MqPwCiU=",
            "chain_id": 1
            },
            {
            "emitter": "+M0jwquRI3cwdwu+oI1hAFzdoJhDSPP27stVljjAu6A=",
            "chain_id": 26
            }
        ],
        "governance_source": {
            "emitter": "VjWXmiIcNJMeMmILkpOkYwZVVepx/pfNYjet6HWxLp4=",
            "chain_id": 1
        }
    });
    let pyth_oracle = pyth_oracle
        .instantiate(
            &basic.wallet,
            "Test Pyth Contract",
            vec![],
            oracle_init_msg,
            cosmos::ContractAdmin::Sender,
        )
        .await?;
    log::info!("Deployed new Pyth oracle contract: {pyth_oracle}");

    Ok(())
}
