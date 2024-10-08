use std::collections::HashSet;

use cosmos::proto::cosmwasm::wasm::v1::MsgExecuteContract;
use cosmos::{Contract, HasAddress};
use cosmwasm_std::{Binary, Coin};
use perpswap::prelude::*;
use pyth_sdk_cw::PriceIdentifier;

pub async fn get_oracle_update_msg(
    ids: &HashSet<PriceIdentifier>,
    sender: impl HasAddress,
    endpoint: &reqwest::Url,
    client: &reqwest::Client,
    oracle: &Contract,
) -> Result<MsgExecuteContract> {
    let vaas = get_wormhole_proofs(ids, endpoint, client).await?;
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
    ids: &HashSet<PriceIdentifier>,
    endpoint: &reqwest::Url,
    client: &reqwest::Client,
) -> Result<Vec<String>> {
    anyhow::ensure!(
        !ids.is_empty(),
        "Cannot get wormhole proofs with no price IDs"
    );
    #[derive(serde::Deserialize)]
    struct PythResponse {
        binary: PythData,
    }

    #[derive(serde::Deserialize)]
    struct PythData {
        data: Vec<String>,
    }

    let url_params = ids.iter().map(|id| ("ids[]", id.to_hex()));
    let url_params = url_params.chain([
        ("parsed", "false".to_owned()),
        ("encoding", "base64".to_owned()),
    ]);
    let url = endpoint.join("v2/updates/price/latest")?;
    let url = reqwest::Url::parse_with_params(url.as_str(), url_params)?;

    let response: PythResponse = fetch_json_with_retry(|| client.get(url.clone())).await?;
    Ok(response.binary.data)
}

pub async fn fetch_json_with_retry<T, F>(make_req: F) -> Result<T>
where
    F: Fn() -> reqwest::RequestBuilder,
    T: serde::de::DeserializeOwned,
{
    const DELAYS_MILLIS: [u64; 5] = [100, 200, 400, 800, 1600];
    let mut attempt = 0;
    loop {
        let req = make_req();
        let res = async move { req.send().await?.error_for_status()?.json().await }.await;
        match res {
            Ok(x) => break Ok(x),
            Err(e) => match DELAYS_MILLIS.get(attempt) {
                Some(delay) => {
                    attempt += 1;
                    tracing::warn!("Error on HTTP request, sleeping {delay}ms and retring. Attempt {attempt}/{}. Error: {e:?}.", DELAYS_MILLIS.len());
                    tokio::time::sleep(tokio::time::Duration::from_millis(*delay)).await;
                }
                None => break Err(e.into()),
            },
        }
    }
}
