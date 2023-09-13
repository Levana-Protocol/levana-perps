mod list_contracts;
mod lp_history;
mod token_balances;

use std::{collections::HashSet, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cosmos::{Address, CosmosNetwork, HasAddress, TxBuilder};
use msg::contracts::market::{
    entry::{PositionAction, PositionActionKind, TradeHistorySummary},
    position::{ClosedPosition, PositionId, PositionQueryResponse, PositionsResp}, spot_price::{PythPriceServiceNetwork, SpotPriceFeedData},
};
use parking_lot::Mutex;
use perps_exes::{
    config::{ChainConfig},
    prelude::MarketContract,
    pyth::{get_oracle_update_msg, VecWithCurr},
};
use serde_json::json;
use shared::storage::{
    Collateral, DirectionToBase, LeverageToBase, MarketId, Notional, Signed, UnsignedDecimal, Usd,
};
use tokio::task::JoinSet;

use crate::factory::Factory;

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
    /// Get the trade volume for a market
    TradeVolume {
        #[clap(flatten)]
        inner: TradeVolumeOpt,
    },
    /// Export a CSV with stats on all positioned opened
    OpenPositionCsv {
        #[clap(flatten)]
        inner: OpenPositionCsvOpt,
    },
    /// Export a CSV with stats on LP actions
    LpActionCsv {
        #[clap(flatten)]
        inner: lp_history::LpActionCsvOpt,
    },
    /// Get token balances based on open position and LP action CSVs
    TokenBalances {
        #[clap(flatten)]
        inner: token_balances::TokenBalancesOpt,
    },
    /// List contracts for the given factories
    ListContracts {
        #[clap(flatten)]
        inner: list_contracts::ListContractsOpt,
    },
}

impl UtilOpt {
    pub(crate) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        match self.sub {
            Sub::UpdatePyth { inner } => update_pyth(opt, inner).await,
            Sub::DeployPyth { inner } => deploy_pyth_opt(opt, inner).await,
            Sub::TradeVolume { inner } => trade_volume(opt, inner).await,
            Sub::OpenPositionCsv { inner } => open_position_csv(opt, inner).await,
            Sub::LpActionCsv { inner } => inner.go(opt).await,
            Sub::TokenBalances { inner } => inner.go(opt).await,
            Sub::ListContracts { inner } => inner.go().await,
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
        config_pyth,
        config_chain,
    }: UpdatePythOpt,
) -> Result<()> {
    let chain = ChainConfig::load(config_chain, network)?;
    let pyth = chain.spot_price.and_then(|spot_price| spot_price.pyth).context("No Pyth oracle found for network {network}")?;
    let basic = opt.load_basic_app(network).await?;

    let oracle_info = opt.get_oracle_info(&basic.chain_config, &basic.price_config, &network.to_string())?;

    // FIXME
    let endpoints = VecWithCurr::new(match pyth.r#type {
        PythPriceServiceNetwork::Stable => basic.price_config.pyth.stable.endpoints.clone(),
        PythPriceServiceNetwork::Edge => basic.price_config.pyth.edge.endpoints.clone()
    });

    let client = reqwest::Client::new();
    // FIXME
    let market = oracle_info.markets
        .get(&market)
        .with_context(|| format!("No oracle feed data found for {market}"))?;

    let oracle = basic.cosmos.make_contract(pyth.contract);

    let ids = market.feeds.iter().chain(market.feeds_usd.iter()).filter_map(|feed| match feed.data {
        SpotPriceFeedData::Pyth { id } => Some(id),
        _ => None,
    }).collect::<Vec<_>>();

    let msg = get_oracle_update_msg(&ids, &basic.wallet, &endpoints, &client, &oracle).await?;

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

#[derive(clap::Parser)]
struct TradeVolumeOpt {
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: CosmosNetwork,
    /// Market address
    market: Address,
}

async fn trade_volume(
    opt: crate::cli::Opt,
    TradeVolumeOpt { network, market }: TradeVolumeOpt,
) -> Result<()> {
    let cosmos = opt.connect(network).await?;
    let contract = MarketContract::new(cosmos.make_contract(market));

    let mut traders = HashSet::<Address>::new();
    let mut next_position_id: PositionId = "1".parse()?;

    loop {
        match contract.raw_query_positions(vec![next_position_id]).await {
            Ok(PositionsResp {
                positions,
                pending_close,
                closed,
            }) => {
                anyhow::ensure!(1 == positions.len() + pending_close.len() + closed.len());

                for pos in &positions {
                    println!("{},{},open", pos.id, pos.notional_size);
                }
                anyhow::ensure!(pending_close.is_empty());
                for pos in &closed {
                    println!("{},{},closed", pos.id, pos.notional_size);
                }

                positions
                    .into_iter()
                    .map(|x| x.owner)
                    .chain(pending_close.into_iter().map(|x| x.owner))
                    .chain(closed.into_iter().map(|x| x.owner))
                    .try_for_each(|addr| {
                        addr.into_string()
                            .parse()
                            .context("Invalid trader address")
                            .map(|addr| {
                                traders.insert(addr);
                            })
                    })?;
                next_position_id = (next_position_id.u64() + 1).to_string().parse()?;
            }
            Err(e) => {
                log::warn!("Make sure this says that the position isn't found: {e:?}");
                break;
            }
        }
    }

    log::info!("Last position checked: {next_position_id}");
    log::info!("Total traders: {}", traders.len());

    let mut total_trade_volume = Usd::zero();
    let mut total_realized_pnl = Signed::<Usd>::zero();

    for trader in traders {
        let TradeHistorySummary {
            trade_volume,
            realized_pnl,
        } = contract.trade_history_summary(trader).await?;
        total_trade_volume = total_trade_volume.checked_add(trade_volume)?;
        total_realized_pnl = total_realized_pnl.checked_add(realized_pnl)?;
    }

    log::info!("Total trade volume: {total_trade_volume}");
    log::info!("Total realized PnL: {total_realized_pnl}");
    Ok(())
}

#[derive(clap::Parser)]
struct OpenPositionCsvOpt {
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: CosmosNetwork,
    /// Factory address
    #[clap(long)]
    factory: Address,
    /// Output CSV file
    #[clap(long)]
    csv: PathBuf,
    /// How many separate worker tasks to create for parallel loading
    #[clap(long, default_value = "30")]
    workers: u32,
}

struct ToProcess {
    next: PositionId,
    last: PositionId,
    market: MarketContract,
    market_id: Arc<MarketId>,
}

async fn open_position_csv(
    opt: crate::cli::Opt,
    OpenPositionCsvOpt {
        network,
        factory,
        csv,
        workers,
    }: OpenPositionCsvOpt,
) -> Result<()> {
    let cosmos = opt.connect(network).await?;
    let factory = Factory::from_contract(cosmos.make_contract(factory));
    let csv = ::csv::Writer::from_path(&csv)?;
    let csv = Arc::new(Mutex::new(csv));

    let markets = factory.get_markets().await?;

    let mut to_process = Vec::<ToProcess>::new();

    for market in markets {
        let market_id = market.market_id.into();
        let market = MarketContract::new(market.market);
        to_process.push(ToProcess {
            next: "1".parse()?,
            last: market.get_highest_position_id().await?,
            market,
            market_id,
        });
    }

    let to_process = Arc::new(Mutex::new(to_process));

    let mut set = JoinSet::new();

    for _ in 0..workers {
        let to_process = to_process.clone();
        let csv = csv.clone();
        set.spawn(csv_helper(to_process, csv));
    }

    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(())) => (),
            Ok(Err(e)) => {
                set.abort_all();
                return Err(e);
            }
            Err(e) => {
                set.abort_all();
                return Err(e).context("Unexpected panic");
            }
        }
    }

    Ok(())
}

async fn csv_helper(
    to_process: Arc<Mutex<Vec<ToProcess>>>,
    csv: Arc<Mutex<csv::Writer<std::fs::File>>>,
) -> Result<()> {
    loop {
        let (contract, market_id, pos_id) = {
            let mut to_process_guard = to_process.lock();
            match to_process_guard.last_mut() {
                None => break Ok(()),
                Some(to_process) => {
                    if to_process.next > to_process.last {
                        to_process_guard.pop();
                        continue;
                    }

                    let pos_id = to_process.next;
                    to_process.next = (pos_id.u64() + 1).to_string().parse()?;
                    (
                        to_process.market.clone(),
                        to_process.market_id.clone(),
                        pos_id,
                    )
                }
            }
        };

        let PositionAction {
            id,
            kind,
            timestamp,
            collateral: _,
            transfer_collateral: _,
            leverage,
            max_gains: _,
            trade_fee: _,
            delta_neutrality_fee: _,
            old_owner: _,
            new_owner: _,
            take_profit_override: _,
            stop_loss_override: _,
        } = contract
            .first_position_action(pos_id)
            .await?
            .context("Impossible missing first action for a position")?;
        anyhow::ensure!(kind == PositionActionKind::Open);
        anyhow::ensure!(id == Some(pos_id));

        let timestamp = timestamp.try_into_chrono_datetime()?;
        let leverage = leverage
            .with_context(|| format!("Missing leverage on position open action for {pos_id}"))?;

        let PositionsResp {
            positions,
            pending_close,
            closed,
        } = contract.raw_query_positions(vec![pos_id]).await?;

        let common = PositionRecordCommon {
            market: &market_id,
            id: pos_id,
            timestamp,
            leverage,
        };

        let record = if let Some(position) = positions.first() {
            PositionRecord::from_open(common, position)
        } else if let Some(position) = pending_close.first() {
            PositionRecord::from_closed(common, position)
        } else if let Some(position) = closed.first() {
            PositionRecord::from_closed(common, position)
        } else {
            anyhow::bail!("Could not find position {pos_id}");
        }?;

        let mut csv = csv.lock();
        csv.serialize(&record)?;
        csv.flush()?;
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum PositionStatus {
    Open,
    Closed,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct PositionRecordCommon<'a> {
    market: &'a MarketId,
    id: PositionId,
    timestamp: DateTime<Utc>,
    leverage: LeverageToBase,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct PositionRecord<'a> {
    market: &'a MarketId,
    id: PositionId,
    opened_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    leverage: LeverageToBase,
    owner: Address,
    direction: DirectionToBase,
    status: PositionStatus,
    deposit_collateral: Signed<Collateral>,
    deposit_collateral_usd: Signed<Usd>,
    active_collateral: Collateral,
    notional_size: Signed<Notional>,
    pnl_collateral: Signed<Collateral>,
    pnl_usd: Signed<Usd>,
    total_fees_collateral: Signed<Collateral>,
    total_fees_usd: Signed<Usd>,
}

impl<'a> PositionRecord<'a> {
    fn from_open(
        PositionRecordCommon {
            market,
            id,
            timestamp,
            leverage,
        }: PositionRecordCommon<'a>,
        position: &'a PositionQueryResponse,
    ) -> Result<Self> {
        let total_fees_collateral = position.borrow_fee_collateral.into_signed()
            + position.funding_fee_collateral
            + position.crank_fee_collateral.into_signed()
            + position.trading_fee_collateral.into_signed()
            + position.delta_neutrality_fee_collateral;
        let total_fees_usd = position.borrow_fee_usd.into_signed()
            + position.funding_fee_usd
            + position.crank_fee_usd.into_signed()
            + position.trading_fee_usd.into_signed()
            + position.delta_neutrality_fee_usd;
        Ok(Self {
            market,
            id,
            opened_at: timestamp,
            closed_at: None,
            leverage,
            owner: position.owner.as_str().parse()?,
            direction: position.direction_to_base,
            deposit_collateral: position.deposit_collateral,
            deposit_collateral_usd: position.deposit_collateral_usd,
            notional_size: position.notional_size,
            pnl_collateral: position.pnl_collateral,
            pnl_usd: position.pnl_usd,
            status: PositionStatus::Open,
            total_fees_collateral,
            total_fees_usd,
            active_collateral: position.active_collateral.raw(),
        })
    }

    fn from_closed(
        PositionRecordCommon {
            market,
            id,
            timestamp,
            leverage,
        }: PositionRecordCommon<'a>,
        position: &'a ClosedPosition,
    ) -> Result<Self> {
        let total_fees_collateral = position.borrow_fee_collateral.into_signed()
            + position.funding_fee_collateral
            + position.crank_fee_collateral.into_signed()
            + position.trading_fee_collateral.into_signed()
            + position.delta_neutrality_fee_collateral;
        let total_fees_usd = position.borrow_fee_usd.into_signed()
            + position.funding_fee_usd
            + position.crank_fee_usd.into_signed()
            + position.trading_fee_usd.into_signed()
            + position.delta_neutrality_fee_usd;
        Ok(Self {
            market,
            id,
            opened_at: timestamp,
            closed_at: Some(position.close_time.try_into_chrono_datetime()?),
            leverage,
            owner: position.owner.as_str().parse()?,
            direction: position.direction_to_base,
            deposit_collateral: position.deposit_collateral,
            deposit_collateral_usd: position.deposit_collateral_usd,
            notional_size: position.notional_size,
            pnl_collateral: position.pnl_collateral,
            pnl_usd: position.pnl_usd,
            status: PositionStatus::Closed,
            total_fees_collateral,
            total_fees_usd,
            active_collateral: Collateral::zero(),
        })
    }
}
