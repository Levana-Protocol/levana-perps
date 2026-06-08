use anyhow::Result;
use cosmos::{Address, HasAddress};
use cosmwasm_std::Decimal256;
use perps_exes::{config::MainnetFactories, contracts::Factory};
use perpswap::{
    contracts::{
        cw20::entry::{BalanceResponse, QueryMsg as Cw20QueryMsg},
        market::entry::{QueryMsg as MarketQueryMsg, StatusResp},
    },
    token::Token,
};

#[derive(clap::Parser)]
pub(super) struct MarketFundsOpts {
    /// Factory contract addresses or identifiers
    #[clap(long, required = true, value_delimiter = ',')]
    factories: Vec<String>,
}

impl MarketFundsOpts {
    pub(super) async fn go(self, opt: crate::cli::Opt) -> Result<()> {
        go(opt, self).await
    }
}

async fn go(opt: crate::cli::Opt, MarketFundsOpts { factories }: MarketFundsOpts) -> Result<()> {
    let mainnet_factories = MainnetFactories::load()?;

    println!("factory,network,market,contract,collateral,raw_amount,amount");

    for factory in factories {
        let mainnet_factory = mainnet_factories.get(&factory)?;
        let app = opt.load_app_mainnet(mainnet_factory.network).await?;
        let factory_contract =
            Factory::from_contract(app.cosmos.make_contract(mainnet_factory.address));

        for market in factory_contract.get_markets().await? {
            let status: StatusResp = market
                .market
                .query(MarketQueryMsg::Status { price: None })
                .await?;
            let (collateral, raw_amount, amount) =
                market_balance(&app.cosmos, market.market.get_address(), &status.collateral)
                    .await?;

            println!(
                "{factory},{},{},{},{collateral},{raw_amount},{amount}",
                mainnet_factory.network,
                market.market_id,
                market.market.get_address(),
            );
        }
    }

    Ok(())
}

async fn market_balance(
    cosmos: &cosmos::Cosmos,
    market_addr: Address,
    collateral: &Token,
) -> Result<(String, u128, Decimal256)> {
    let (collateral_label, raw_amount) = match collateral {
        Token::Cw20 { addr, .. } => {
            let cw20 = cosmos.make_contract(addr.as_str().parse()?);
            let BalanceResponse { balance } = cw20
                .query(Cw20QueryMsg::Balance {
                    address: market_addr.get_address_string().into(),
                })
                .await?;
            (addr.to_string(), balance.u128())
        }
        Token::Native { denom, .. } => {
            let raw_amount = cosmos
                .all_balances(market_addr)
                .await?
                .into_iter()
                .find(|coin| coin.denom == *denom)
                .map(|coin| coin.amount.parse())
                .transpose()?
                .unwrap_or(0);
            (denom.clone(), raw_amount)
        }
    };
    let amount = collateral.from_u128(raw_amount)?;

    Ok((collateral_label, raw_amount, amount))
}
