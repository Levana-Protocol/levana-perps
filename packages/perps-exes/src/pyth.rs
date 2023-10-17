use std::collections::HashSet;

use cosmos::proto::cosmwasm::wasm::v1::MsgExecuteContract;
use cosmos::{Contract, HasAddress};
use cosmwasm_std::{Binary, Coin};
use msg::prelude::*;
use pyth_sdk_cw::PriceIdentifier;

pub async fn get_oracle_update_msg(
    ids: &HashSet<PriceIdentifier>,
    sender: impl HasAddress,
    endpoint: &str,
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
    endpoint: &str,
    client: &reqwest::Client,
) -> Result<Vec<String>> {
    // pyth uses this format for array params: https://github.com/axios/axios/blob/9588fcdec8aca45c3ba2f7968988a5d03f23168c/test/specs/helpers/buildURL.spec.js#L31
    let url_params = ids
        .iter()
        .map(|id| format!("ids[]={id}"))
        .collect::<Vec<String>>()
        .join("&");
    let url_params = &url_params;

    let url = format!("{endpoint}api/latest_vaas?{url_params}");

    let vaas: Vec<String> = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(vaas)
}
