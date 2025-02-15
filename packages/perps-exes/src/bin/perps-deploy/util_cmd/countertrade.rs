use std::str::FromStr;

use anyhow::Context;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use cosmos::Address;
use cosmwasm_std::{Binary, Uint128};
use perps_exes::{contracts::Factory, PerpsNetwork};
use perpswap::contracts::{countertrade::MarketStatus, market::entry::StatusResp};
use perpswap::{
    number::Number,
    storage::{MarketId, RawAddr},
};

#[derive(clap::Subcommand)]
pub(crate) enum CounterTradeSub {
    DepositCollateral {
        /// Family name for these contracts
        #[clap(long, env = "PERPS_FAMILY")]
        family: String,
        /// How much amount to deposit
        #[clap(long)]
        amount: Option<Uint128>,
        /// Skip market ids
        #[clap(long, env = "COUNTERTRADE_SKIP_MARKET_IDS", value_delimiter = ',')]
        skip_market_ids: Vec<MarketId>,
        /// Flag to actually execute
        #[clap(long)]
        do_it: bool,
    },
    /// Check if market is balanced
    Stats {
        /// Family name for these contracts
        #[clap(long)]
        factory: Address,
        /// Cosmos network to use
        #[clap(long, env = "COSMOS_NETWORK")]
        cosmos_network: PerpsNetwork,
    },
    /// Collateral and shares details
    Shares {
        /// Countertrade contract Address
        #[clap(long, env = "COUNTERTRADE_CONTRACT_ADDRESS")]
        contract: Address,
        /// Cosmos network to use
        #[clap(long, env = "COSMOS_NETWORK")]
        cosmos_network: PerpsNetwork,
    },
}

impl CounterTradeSub {
    pub(crate) async fn go(self, opt: crate::cli::Opt) -> anyhow::Result<()> {
        go(opt, self).await
    }
}

async fn go(opt: crate::cli::Opt, sub: CounterTradeSub) -> anyhow::Result<()> {
    match sub {
        CounterTradeSub::DepositCollateral {
            family,
            do_it,
            amount,
            skip_market_ids,
        } => deposit_collateral(opt, family, do_it, amount, skip_market_ids).await?,
        CounterTradeSub::Stats {
            factory,
            cosmos_network,
        } => {
            let cosmos = opt.connect(cosmos_network).await?;
            let factory = cosmos.make_contract(factory);
            let factory = Factory::from_contract(factory);
            let markets = factory.get_markets().await?;
            struct FundingResult {
                popular_funding: Number,
                market_id: MarketId,
            }
            let mut results = vec![];
            for market in markets {
                let status = perpswap::contracts::market::entry::QueryMsg::Status { price: None };
                let market_contract = market.market;
                let status: StatusResp = market_contract.query(status).await?;
                let popular_funding = basic_market_analysis(&status)?;
                results.push(FundingResult {
                    popular_funding,
                    market_id: market.market_id,
                });
            }
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(comfy_table::ContentArrangement::Dynamic)
                .set_width(80)
                .set_header(vec![Cell::new("Market ID"), Cell::new("Popular Funding")]);
            for item in results {
                table.add_row(vec![
                    item.market_id.to_string(),
                    item.popular_funding.to_string(),
                ]);
            }
            println!("{table}");
        }
        CounterTradeSub::Shares {
            contract,
            cosmos_network,
        } => {
            let cosmos = opt.connect(cosmos_network).await?;
            let msg = perpswap::contracts::countertrade::QueryMsg::Status {};
            let contract = cosmos.make_contract(contract);
            let response: MarketStatus = contract.query(msg.clone()).await?;
            shares_analysis(vec![response])?;
        }
    }
    Ok(())
}

fn shares_analysis(status: Vec<MarketStatus>) -> anyhow::Result<()> {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(comfy_table::ContentArrangement::Dynamic)
        .set_width(80)
        .set_header(vec![
            Cell::new("Market ID"),
            Cell::new("Collateral"),
            Cell::new("Shares"),
            Cell::new("Position collateral"),
        ]);
    for item in status {
        table.add_row(vec![
            item.id.to_string(),
            item.collateral.to_string(),
            item.shares.to_string(),
            item.position
                .map(|item| item.active_collateral.to_string())
                .unwrap_or("NA".to_owned()),
        ]);
    }
    println!("{table}");
    Ok(())
}

fn basic_market_analysis(status: &StatusResp) -> anyhow::Result<Number> {
    let (long_funding, short_funding) = match status.market_type {
        perpswap::storage::MarketType::CollateralIsQuote => {
            (status.long_funding, status.short_funding)
        }
        perpswap::storage::MarketType::CollateralIsBase => {
            (status.short_funding, status.long_funding)
        }
    };
    let popular_funding = if long_funding.is_strictly_positive() {
        long_funding
    } else {
        short_funding
    };
    Ok(popular_funding)
}

async fn deposit_collateral(
    opt: crate::cli::Opt,
    family: String,
    do_it: bool,
    amount: Option<Uint128>,
    skip_market_ids: Vec<MarketId>,
) -> anyhow::Result<()> {
    let app = opt.load_app(&family).await?;
    let factory = app.tracker.get_factory(&family).await?.into_contract();
    let cosmos = app.basic.cosmos.clone();
    let wallet = app.basic.get_wallet()?;

    let factory = Factory::from_contract(factory);
    let markets = factory.get_markets().await?;
    let contracts = factory.get_countertrade_address().await?;

    let amount = match amount {
        Some(amount) => amount,
        None => Uint128::from(1000000000u128),
    };
    for (market_id, counter_trade) in contracts {
        if skip_market_ids.contains(&market_id) {
            println!("Skipping Market {market_id}");
            continue;
        } else {
            println!("Processing Market {market_id}");
        }

        let market = markets
            .iter()
            .find(|item| item.market_id == market_id)
            .context(format!("Market {market_id} not found in factory"))?;
        let market = market.market.clone();
        let market_status = perpswap::contracts::market::entry::QueryMsg::Status { price: None };
        let response: StatusResp = market.query(market_status).await?;
        let contract = Address::from_str(counter_trade.as_str())?;

        match response.collateral {
            perpswap::token::Token::Cw20 { addr, .. } => {
                println!("Cw20 Contract: {addr}");
                let deposit_msg = perpswap::contracts::countertrade::ExecuteMsg::Deposit {};
                let deposit_msg = Binary::new(serde_json::to_vec(&deposit_msg)?);
                let cw20_execute_msg = perpswap::contracts::cw20::entry::ExecuteMsg::Send {
                    contract: RawAddr::from(contract.to_string()),
                    amount,
                    msg: deposit_msg,
                };
                let cw20_execute_msg_str = serde_json::to_string(&cw20_execute_msg)?;
                println!("Cw20 Message: {cw20_execute_msg_str:?}");

                if do_it {
                    tracing::info!("Executing");
                    let cw20_contract = cosmos.make_contract(Address::from_str(addr.as_str())?);
                    let response = cw20_contract
                        .execute(wallet, vec![], cw20_execute_msg)
                        .await?;
                    println!("Txhash: {}", response.txhash);
                }
            }
            perpswap::token::Token::Native { denom, .. } => {
                println!("Countertrade contract: {contract}");
                let deposit_msg = perpswap::contracts::countertrade::ExecuteMsg::Deposit {};
                let deposit_msg = serde_json::to_string(&deposit_msg)?;
                println!("Message: {deposit_msg}");
                if do_it {
                    tracing::info!("Executing");
                    let countertrade = cosmos.make_contract(contract);
                    let response = countertrade
                        .execute(
                            wallet,
                            vec![cosmos::Coin {
                                denom,
                                amount: amount.to_string(),
                            }],
                            deposit_msg,
                        )
                        .await?;
                    println!("Txhash: {}", response.txhash);
                }
            }
        }
    }

    Ok(())
}
