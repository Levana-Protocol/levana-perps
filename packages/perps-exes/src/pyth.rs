mod vec_with_curr;

use std::collections::HashMap;

use cosmos::proto::cosmwasm::wasm::v1::MsgExecuteContract;
use cosmos::{Contract, HasAddress};
use cosmwasm_std::{Binary, Coin, Decimal256, Uint256};
use itertools::Itertools;
use msg::contracts::pyth_bridge::{PythMarketPriceFeeds, PythPriceFeed};
use msg::prelude::*;
use shared::storage::{PriceBaseInQuote, PriceCollateralInUsd};
pub use vec_with_curr::VecWithCurr;

pub async fn get_oracle_update_msg(
    feeds: &PythMarketPriceFeeds,
    sender: impl HasAddress,
    endpoints: &VecWithCurr<String>,
    client: &reqwest::Client,
    oracle: &Contract,
) -> Result<MsgExecuteContract> {
    let vaas = get_wormhole_proofs(feeds, endpoints, client).await?;
    let vaas_binary = vaas
        .iter()
        .map(|vaa| Binary::from_base64(vaa).map_err(|err| err.into()))
        .collect::<Result<Vec<_>>>()?;

    let fees: Coin = oracle
        .query(pyth_sdk_cw::QueryMsg::GetUpdateFee {
            vaas: vaas_binary.clone(),
        })
        .await?;

    Ok(MsgExecuteContract {
        sender: sender.get_address_string(),
        contract: oracle.get_address_string(),
        msg: serde_json::to_vec(&pyth_sdk_cw::ExecuteMsg::UpdatePriceFeeds { data: vaas_binary })?,
        funds: vec![cosmos::Coin {
            denom: fees.denom,
            amount: fees.amount.to_string(),
        }],
    })
}

async fn get_wormhole_proofs(
    market_price_feeds: &PythMarketPriceFeeds,
    endpoints: &VecWithCurr<String>,
    client: &reqwest::Client,
) -> Result<Vec<String>> {
    let mut all_ids = market_price_feeds
        .feeds
        .iter()
        .chain(market_price_feeds.feeds_usd.as_deref().unwrap_or_default())
        .map(|f| f.id)
        .sorted()
        .dedup()
        .collect_vec();

    all_ids.sort();
    all_ids.dedup();
    let all_ids_len = all_ids.len();
    // pyth uses this format for array params: https://github.com/axios/axios/blob/9588fcdec8aca45c3ba2f7968988a5d03f23168c/test/specs/helpers/buildURL.spec.js#L31
    let url_params = all_ids
        .iter()
        .map(|id| format!("ids[]={id}"))
        .collect::<Vec<String>>()
        .join("&");
    let url_params = &url_params;

    endpoints
        .try_any_from_curr_async(|endpoint| async move {
            let url = format!("{endpoint}api/latest_vaas?{url_params}");

            let vaas: Vec<String> = client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            if vaas.len() != all_ids_len {
                anyhow::bail!("expected {} vaas, got {}", all_ids_len, vaas.len());
            }
            Ok(vaas)
        })
        .await
}

pub async fn get_latest_price(
    client: &reqwest::Client,
    market_price_feeds: &PythMarketPriceFeeds,
    endpoints: &VecWithCurr<String>,
) -> Result<(PriceBaseInQuote, Option<PriceCollateralInUsd>)> {
    endpoints
        .try_any_from_curr_async(|endpoint| async move {
            let base = price_helper(client, &endpoint, &market_price_feeds.feeds).await?;
            let base = PriceBaseInQuote::try_from_number(base.into_signed())?;

            let collateral = match &market_price_feeds.feeds_usd {
                Some(feeds_usd) => {
                    let collateral = price_helper(client, &endpoint, feeds_usd).await?;
                    Some(PriceCollateralInUsd::try_from_number(
                        collateral.into_signed(),
                    ))
                }
                None => None,
            }
            .transpose()?;

            Ok((base, collateral))
        })
        .await
}

async fn price_helper(
    client: &reqwest::Client,
    endpoint: &str,
    feeds: &[PythPriceFeed],
) -> Result<Decimal256> {
    let mut req = client.get(format!("{}api/latest_price_feeds", endpoint));
    for feed in feeds {
        req = req.query(&[("ids[]", feed.id)]);
    }

    #[derive(serde::Deserialize)]
    struct Record {
        id: String,
        price: Price,
    }
    #[derive(serde::Deserialize)]
    struct Price {
        expo: i8,
        price: Uint256,
    }
    let records = req
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Record>>()
        .await?;

    let prices = records
        .into_iter()
        .map(|Record { id, price }| (id, price))
        .collect::<HashMap<_, _>>();

    let mut final_price = Decimal256::one();

    for feed in feeds {
        let Price { expo, price } = prices
            .get(&feed.id.to_hex())
            .with_context(|| format!("Missing price for ID {}", feed.id))?;

        anyhow::ensure!(*expo <= 0, "Exponent from Pyth must always be negative");
        let component = Decimal256::from_atomics(*price, expo.abs().try_into()?)?;
        if feed.inverted {
            final_price = final_price.checked_div(component)?;
        } else {
            final_price = final_price.checked_mul(component)?;
        }
    }

    Ok(final_price)
}
