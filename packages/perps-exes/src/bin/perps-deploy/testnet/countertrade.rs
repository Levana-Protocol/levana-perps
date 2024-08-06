use std::str::FromStr;

use anyhow::Context;
use cosmos::Address;
use cosmwasm_std::Binary;
use msg::contracts::market::entry::StatusResp;
use perps_exes::contracts::Factory;
use shared::storage::{MarketId, RawAddr};

#[derive(clap::Subcommand)]
pub(crate) enum CounterTradeSub {
    DepositCollateral {
        /// Countertrade contract Address
        #[clap(long, env = "COUNTERTRADE_CONTRACT_ADDRESS")]
        contract: Address,
        /// Family name for these contracts
        #[clap(long, env = "PERPS_FAMILY")]
        family: String,
        /// Which market to deposit collateral for
        #[clap(long)]
        market_id: MarketId,
        /// Flag to actually execute
        #[clap(long)]
        do_it: bool,
    },
    // Check if market is balanced
    Stats {
        /// Family name for these contracts
        #[clap(long, env = "PERPS_FAMILY")]
        family: String,
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
            contract,
            family,
            market_id,
            do_it,
        } => deposit_collateral(opt, contract, family, market_id, do_it).await?,
        CounterTradeSub::Stats { family } => {
            let app = opt.load_app(&family).await?;
            let factory = app.tracker.get_factory(&family).await?.into_contract();
            let factory = Factory::from_contract(factory);
            let markets = factory.get_markets().await?;
            for market in markets {
                let status = msg::contracts::market::entry::QueryMsg::Status { price: None };
                let market_contract = market.market;
                let status: StatusResp = market_contract.query(status).await?;
                basic_market_analysis(&market.market_id, &status)?;
            }
        }
    }
    Ok(())
}

fn basic_market_analysis(market_id: &MarketId, status: &StatusResp) -> anyhow::Result<()> {
    let (long_funding, short_funding) = match status.market_type {
        shared::storage::MarketType::CollateralIsQuote => {
            (status.long_funding, status.short_funding)
        }
        shared::storage::MarketType::CollateralIsBase => {
            (status.short_funding, status.long_funding)
        }
    };
    let popular_funding = if long_funding.is_strictly_positive() {
        long_funding
    } else {
        short_funding
    };
    // Todo: Fetch min_funding and max_funding on-chain
    println!("{market_id}: min_funding: 0.1, popular_funding: {popular_funding}, max_funding: 0.6",);
    Ok(())
}

async fn deposit_collateral(
    opt: crate::cli::Opt,
    contract: Address,
    family: String,
    market_id: MarketId,
    do_it: bool,
) -> anyhow::Result<()> {
    let app = opt.load_app(&family).await?;
    let factory = app.tracker.get_factory(&family).await?.into_contract();
    let cosmos = app.basic.cosmos.clone();
    let wallet = app.basic.get_wallet()?;

    let factory = Factory::from_contract(factory);
    let markets = factory.get_markets().await?;
    let market = markets
        .into_iter()
        .find(|item| item.market_id == market_id)
        .context(format!("Market {market_id} not found in factory"))?;

    let market = market.market;

    let market_status = msg::contracts::market::entry::QueryMsg::Status { price: None };
    let response: StatusResp = market.query(market_status).await?;

    match response.collateral {
        msg::token::Token::Cw20 { addr, .. } => {
            println!("Cw20 Contract: {addr}");
            let deposit_msg =
                msg::contracts::countertrade::ExecuteMsg::Deposit { market: market_id };
            let deposit_msg = Binary::new(serde_json::to_vec(&deposit_msg)?);
            let cw20_execute_msg = msg::contracts::cw20::entry::ExecuteMsg::Send {
                contract: RawAddr::from(contract.to_string()),
                amount: "1000000000"
                    .parse()
                    .context("Error converting 1000000000 into Uint128")?,
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
        msg::token::Token::Native { denom, .. } => {
            println!("Countertrade contract: {contract}");
            let deposit_msg =
                msg::contracts::countertrade::ExecuteMsg::Deposit { market: market_id };
            let deposit_msg = serde_json::to_string(&deposit_msg)?;
            println!("Message: {deposit_msg}");
            if do_it {
                tracing::info!("Executing");
                let countertrade = cosmos.make_contract(contract);
                let amount = "1000000000"
                    .parse()
                    .context("Error convert 1000000000 into Uint128")?;
                let response = countertrade
                    .execute(wallet, vec![cosmos::Coin { denom, amount }], deposit_msg)
                    .await?;
                println!("Txhash: {}", response.txhash);
            }
        }
    }

    Ok(())
}
