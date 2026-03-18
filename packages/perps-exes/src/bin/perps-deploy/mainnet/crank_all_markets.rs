use anyhow::Result;
use perps_exes::{config::MainnetFactories, contracts::Factory};
use perpswap::{
    contracts::market::entry::StatusResp,
    storage::{MarketExecuteMsg, MarketQueryMsg},
};

#[derive(clap::Parser)]
pub(super) struct CrankAllMarketsOpts {
    /// The factory contract address or identifier
    #[clap(long)]
    factory: String,
}

impl CrankAllMarketsOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(
    opt: crate::cli::Opt,
    CrankAllMarketsOpts { factory }: CrankAllMarketsOpts,
) -> Result<()> {
    let factories = MainnetFactories::load()?;
    let factory = factories.get(&factory)?;

    let app = opt.load_app_mainnet(factory.network).await?;
    let factory = Factory::from_contract(app.cosmos.make_contract(factory.address));

    let markets = factory.get_markets().await?;
    let wallet = app.get_wallet()?;

    for market in markets {
        crank_market(wallet, market).await?;
    }
    Ok(())
}

async fn crank_market(
    wallet: &cosmos::Wallet,
    market: perps_exes::contracts::MarketInfo,
) -> Result<()> {
    tracing::info!("Cranking market {}", market.market_id);
    loop {
        let StatusResp { next_crank, .. } = market
            .market
            .query(MarketQueryMsg::Status { price: None })
            .await?;
        if next_crank.is_none() {
            break Ok(());
        }
        market
            .market
            .execute(
                wallet,
                vec![],
                MarketExecuteMsg::Crank {
                    execs: None,
                    rewards: None,
                },
            )
            .await?;
        println!("Finished one crank");
    }
}
//         match market
//             .market
//             .execute(
//                 app.get_wallet()?,
//                 vec![],
//                 perpswap::contracts::market::entry::ExecuteMsg::TransferDaoFees {},
//             )
//             .await
//         {
//             Ok(tx) => {
//                 tracing::info!(
//                     "Transferred fees from market {} to DAO: {}",
//                     market.market_id,
//                     tx.txhash
//                 );
//             }
//             Err(err) => {
//                 if err
//                     .to_string()
//                     .contains("No DAO fees available to transfer")
//                 {
//                     tracing::info!("No DAO fees available on {}", market.market_id);
//                 } else {
//                     return Err(err.into());
//                 }
//             }
//         }
//     }

//     Ok(())
// }
