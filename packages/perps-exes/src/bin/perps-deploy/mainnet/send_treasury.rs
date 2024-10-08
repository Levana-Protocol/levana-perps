use std::collections::HashMap;

use anyhow::{Context, Result};
use cosmos::{HasAddress, TxBuilder};
use cosmwasm_std::CosmosMsg;
use perpswap::{
    contracts::market::entry::{QueryMsg, StatusResp},
    token::Token,
};
use perps_exes::{config::MainnetFactories, contracts::Factory};

use crate::util::add_cosmos_msg;
use cosmos::Address;

#[derive(clap::Parser)]
pub(super) struct SendTreasuryOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// The destination wallet to receive the funds
    #[clap(long)]
    dest: Address,
}

impl SendTreasuryOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    SendTreasuryOpts { factory, dest }: SendTreasuryOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));
    let treasury = factory.query_dao().await?;
    let balances = app.cosmos.all_balances(treasury).await?;

    if balances.is_empty() {
        tracing::info!("No funds in treasury wallet {treasury}");
        return Ok(());
    }

    let mut sends = vec![];

    for cosmos::Coin { denom, amount } in &balances {
        // Do individual messages, have run into bugs trying to send multiple coins at once
        sends.push(CosmosMsg::Bank(cosmwasm_std::BankMsg::Send {
            to_address: dest.get_address_string(),
            amount: vec![cosmwasm_std::Coin {
                denom: denom.clone(),
                amount: amount.parse()?,
            }],
        }))
    }

    let markets = factory.get_markets().await?;
    let mut collaterals = HashMap::new();
    for market in markets {
        let status: StatusResp = market
            .market
            .query(QueryMsg::Status { price: None })
            .await?;
        let key = match status.collateral {
            Token::Cw20 { addr, .. } => addr.into_string(),
            Token::Native { denom, .. } => denom,
        };

        let entry = collaterals.entry(key.clone()).or_insert(0);
        if balances.iter().any(|c| c.denom == key) {
            *entry += 1;
        }
    }

    println!("\nNumber of markets per collateral asset: {collaterals:#?}");

    println!("\nTreasury contract: {treasury}");
    println!("\nMessage:\n{}\n", serde_json::to_string(&sends)?);

    let mut builder = TxBuilder::default();
    for send in &sends {
        add_cosmos_msg(&mut builder, treasury, send)?;
    }
    let res = builder
        .simulate(&app.cosmos, &[treasury])
        .await
        .context("Error while simulating")?;
    tracing::info!("Successfully simulated messages");
    tracing::debug!("Simulate response: {res:?}");

    Ok(())
}
