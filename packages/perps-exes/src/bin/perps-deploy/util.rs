use std::path::Path;

use anyhow::{Context, Result};
use cosmos::{Address, TxBuilder};
use cosmwasm_std::{CosmosMsg, Empty, WasmMsg};
use sha2::{Digest, Sha256};

pub(crate) fn get_hash_for_path(path: &Path) -> Result<String> {
    let mut file = fs_err::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

pub(crate) fn add_cosmos_msg(
    builder: &mut TxBuilder,
    sender: Address,
    msg: &CosmosMsg<Empty>,
) -> Result<()> {
    match msg {
        CosmosMsg::Bank(_) => anyhow::bail!("No support for bank"),
        CosmosMsg::Custom(_) => anyhow::bail!("No support for custom"),
        CosmosMsg::Staking(_) => anyhow::bail!("No support for staking"),
        CosmosMsg::Distribution(_) => anyhow::bail!("No support for distribution"),
        CosmosMsg::Stargate { .. } => anyhow::bail!("No support for stargate"),
        CosmosMsg::Ibc(_) => anyhow::bail!("No support for IBC"),
        CosmosMsg::Wasm(wasm) => match wasm {
            WasmMsg::Execute {
                contract_addr,
                msg,
                funds,
            } => builder.add_execute_message_mut(
                contract_addr.parse::<Address>()?,
                sender,
                convert_funds(funds),
                convert_msg(msg)?,
            ),
            WasmMsg::Instantiate { .. } => anyhow::bail!("No support for Instantiate"),
            WasmMsg::Migrate {
                contract_addr,
                new_code_id,
                msg,
            } => builder.add_migrate_message_mut(
                contract_addr.parse::<Address>()?,
                sender,
                *new_code_id,
                convert_msg(msg)?,
            ),
            WasmMsg::UpdateAdmin { .. } => anyhow::bail!("No support for UpdateAdmin"),
            WasmMsg::ClearAdmin { contract_addr } => anyhow::bail!("No support for ClearAdmin"),
            _ => anyhow::bail!("Unknown Wasm variant"),
        },
        CosmosMsg::Gov(_) => anyhow::bail!("No support for gov"),
        _ => anyhow::bail!("Unknown CosmosMsg variant"),
    }
}

fn convert_msg(msg: &cosmwasm_std::Binary) -> Result<serde_json::Value> {
    serde_json::from_slice(&msg.0).context("Unable to convert binary to JSON value")
}

fn convert_funds(funds: &[cosmwasm_std::Coin]) -> Vec<cosmos::Coin> {
    funds
        .iter()
        .map(|cosmwasm_std::Coin { denom, amount }| cosmos::Coin {
            denom: denom.clone(),
            amount: amount.to_string(),
        })
        .collect()
}
