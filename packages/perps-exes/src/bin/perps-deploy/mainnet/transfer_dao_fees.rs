use anyhow::Result;
use perps_exes::{config::MainnetFactories, contracts::Factory};

#[derive(clap::Parser)]
pub(super) struct TransferDaoFeesOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
    /// The market to transfer fees from
    /// if none is supplied, then it will transfer from all markets
    #[clap(long)]
    market_id: Option<String>,
}

impl TransferDaoFeesOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    TransferDaoFeesOpts { factory, market_id }: TransferDaoFeesOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let mut markets = factory.get_markets().await?;
    if let Some(market_id) = market_id {
        markets.retain(|m| m.market_id.as_str() == market_id);
    }

    for market in markets {
        match market
            .market
            .execute(
                app.get_wallet()?,
                vec![],
                msg::contracts::market::entry::ExecuteMsg::TransferDaoFees {},
            )
            .await
        {
            Ok(tx) => {
                tracing::info!(
                    "Transferred fees from market {} to DAO: {}",
                    market.market_id,
                    tx.txhash
                );
            }
            Err(err) => {
                if err
                    .to_string()
                    .contains("No DAO fees available to transfer")
                {
                    tracing::info!("No DAO fees available on {}", market.market_id);
                } else {
                    return Err(err.into());
                }
            }
        }
    }

    Ok(())
}
