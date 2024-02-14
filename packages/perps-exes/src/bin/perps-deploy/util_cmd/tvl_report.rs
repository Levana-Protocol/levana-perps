use std::path::PathBuf;

use anyhow::Result;
use cosmos::{Coin, HasAddress};
use cosmwasm_std::Decimal256;
use msg::token::Token;
use perps_exes::{
    config::MainnetFactories,
    contracts::Factory,
    prelude::{Collateral, MarketContract, MarketId, MarketType, UnsignedDecimal, Usd},
};

#[derive(clap::Parser)]
pub(super) struct TvlReportOpt {
    /// Output CSV file
    csv: PathBuf,
}

impl TvlReportOpt {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

#[derive(serde::Serialize)]
struct Record<'a> {
    factory: &'a str,
    market: &'a MarketId,
    collateral: &'a str,
    locked_collateral: Collateral,
    locked_usd: Usd,
    other_coins: Option<String>,
}

async fn go(opt: crate::cli::Opt, TvlReportOpt { csv }: TvlReportOpt) -> Result<()> {
    let mut csv = ::csv::Writer::from_path(&csv)?;

    for factory in ["osmomainnet1", "seimainnet1", "injmainnet1"] {
        go_factory(&mut csv, &opt, factory).await?;
    }

    Ok(())
}

async fn go_factory(
    csv: &mut csv::Writer<std::fs::File>,
    opt: &crate::cli::Opt,
    factory_name: &str,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(factory_name)?;
    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let markets = factory.get_markets().await?;

    for market in markets {
        let market_contract = MarketContract::new(market.market.clone());
        let price = match market_contract.current_price().await {
            Ok(price) => price,
            Err(e) => {
                println!("Skipping market {factory_name}/{}: {e:?}", market.market_id);
                continue;
            }
        };
        let status = market_contract.status().await?;

        let (collateral_denom, collateral_decimal_places) = match &status.collateral {
            Token::Cw20 { .. } => anyhow::bail!(
                "No support for CW20s, found in: {factory_name}/{}",
                status.market_id
            ),
            Token::Native {
                denom,
                decimal_places,
            } => (denom, decimal_places),
        };

        let mut locked_collateral = Collateral::zero();
        let mut other_tokens = vec![];

        for Coin { denom, amount } in app.cosmos.all_balances(market.market.get_address()).await? {
            if &denom == collateral_denom {
                let amount = Decimal256::from_atomics(
                    amount.parse::<u128>()?,
                    (*collateral_decimal_places).into(),
                )?;
                locked_collateral += Collateral::from_decimal256(amount);
            } else {
                other_tokens.push(format!("{amount}{denom}"));
            }
        }

        let locked_usd = price.collateral_to_usd(locked_collateral);

        println!("{factory_name}/{}: {locked_usd}", market.market_id);

        csv.serialize(&Record {
            factory: factory_name,
            market: &market.market_id,
            collateral: match status.market_type {
                MarketType::CollateralIsQuote => &status.quote,
                MarketType::CollateralIsBase => &status.base,
            },
            locked_collateral,
            locked_usd,
            other_coins: if other_tokens.is_empty() {
                None
            } else {
                Some(other_tokens.join(","))
            },
        })?;
    }

    Ok(())
}
