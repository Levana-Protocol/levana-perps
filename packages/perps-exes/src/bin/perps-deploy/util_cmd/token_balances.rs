use std::{collections::HashMap, path::PathBuf};

use cosmos::Address;
use itertools::Itertools;
use msg::prelude::*;
use perps_exes::PerpsNetwork;

use crate::cli::Opt;

#[derive(clap::Parser)]
pub(crate) struct TokenBalancesOpt {
    /// Positions CSV
    #[clap(long)]
    positions_csv: PathBuf,
    /// LP action CSV
    #[clap(long)]
    lp_actions_csv: PathBuf,
    /// Output CSV
    #[clap(long)]
    output_csv: PathBuf,
    /// Network to use.
    #[clap(long, env = "COSMOS_NETWORK")]
    network: PerpsNetwork,
}

impl TokenBalancesOpt {
    pub(super) async fn go(self, opt: Opt) -> Result<()> {
        go(self, opt).await
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct PositionRecord {
    market: MarketId,
    owner: Address,
    active_collateral: Collateral,
}

#[derive(serde::Deserialize)]
struct LpRecord {
    market_id: MarketId,
    addr: Address,
    collateral_remaining: Collateral,
    yield_pending: Collateral,
}

#[derive(serde::Serialize)]
struct OutputRecord {
    wallet: Address,
    market: MarketId,
    trader_collateral: Collateral,
    lp_collateral: Collateral,
    wallet_collateral: Collateral,
}

async fn go(
    TokenBalancesOpt {
        positions_csv,
        lp_actions_csv,
        output_csv,
        network,
    }: TokenBalancesOpt,
    opt: crate::cli::Opt,
) -> Result<()> {
    let positions: Vec<PositionRecord> = csv::Reader::from_path(&positions_csv)?
        .into_deserialize()
        .try_collect()?;
    let lp_actions: Vec<LpRecord> = csv::Reader::from_path(&lp_actions_csv)?
        .into_deserialize()
        .try_collect()?;

    let mut outputs = HashMap::new();

    for PositionRecord {
        market,
        owner,
        active_collateral,
    } in positions
    {
        let record = outputs
            .entry((market, owner))
            .or_insert_with_key(|(market_id, address)| OutputRecord {
                wallet: *address,
                market: market_id.clone(),
                trader_collateral: Collateral::zero(),
                lp_collateral: Collateral::zero(),
                wallet_collateral: Collateral::zero(),
            });
        record.trader_collateral = (record.trader_collateral + active_collateral)?;
    }
    for LpRecord {
        market_id,
        addr,
        collateral_remaining,
        yield_pending,
    } in lp_actions
    {
        let record = outputs
            .entry((market_id, addr))
            .or_insert_with_key(|(market_id, address)| OutputRecord {
                wallet: *address,
                market: market_id.clone(),
                trader_collateral: Collateral::zero(),
                lp_collateral: Collateral::zero(),
                wallet_collateral: Collateral::zero(),
            });
        record.lp_collateral = ((record.lp_collateral + collateral_remaining)? + yield_pending)?;
    }

    let cosmos = opt.connect(network).await?;
    let mut csv = ::csv::Writer::from_path(&output_csv)?;

    for record in outputs.values_mut() {
        let (denom, denominator) = match record.market.as_str() {
            // Load from the factory in the future, being lazy for now
            "ATOM_USD" => (
                "ibc/27394FB092D2ECCD56123C74F36E4C1F926001CEADA9CA97EA622B25F41E5EB2",
                1_000_000u32,
            ),
            "BTC_USD" => (
                "ibc/D1542AA8762DB13087D8364F3EA6509FD6F009A34F00426AF9E4F9FA85CBBF1F",
                100_000_000,
            ),
            market => anyhow::bail!("Unrecognized market: {market}"),
        };

        let balances = cosmos.all_balances(record.wallet).await?;
        tracing::info!("{}: {balances:?}", record.wallet);
        if let Some(coin) = balances.into_iter().find(|coin| coin.denom == denom) {
            let amount = Collateral::from_decimal256(Decimal256::from_ratio(
                coin.amount.parse::<u128>()?,
                denominator,
            ));

            record.wallet_collateral = (record.wallet_collateral + amount)?;
        }
    }

    for record in outputs.values() {
        csv.serialize(record)?;
    }

    Ok(())
}
