use std::path::Path;

use anyhow::{Context, Result};
use cosmos::{proto::cosmos::bank::v1beta1::MsgSend, Address, HasAddress, TxBuilder};
use cosmwasm_std::{BankMsg, CosmosMsg, Empty, WasmMsg};
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
        CosmosMsg::Bank(bank) => match bank {
            BankMsg::Send { to_address, amount } => {
                builder.add_message(MsgSend {
                    from_address: sender.get_address_string(),
                    to_address: to_address.clone(),
                    amount: amount
                        .iter()
                        .map(|cosmwasm_std::Coin { denom, amount }| cosmos::Coin {
                            denom: denom.clone(),
                            amount: amount.to_string(),
                        })
                        .collect(),
                });
                Ok(())
            }
            BankMsg::Burn { amount: _ } => anyhow::bail!("No support for burn"),
            _ => anyhow::bail!("Unknown BankMsg variant"),
        },
        CosmosMsg::Custom(_) => anyhow::bail!("No support for custom"),
        CosmosMsg::Wasm(wasm) => match wasm {
            WasmMsg::Execute {
                contract_addr,
                msg,
                funds,
            } => builder
                .add_execute_message(
                    contract_addr.parse::<Address>()?,
                    sender,
                    convert_funds(funds),
                    convert_msg(msg)?,
                )
                .map_err(|e| e.into())
                .map(|_| ()),
            WasmMsg::Instantiate { .. } => anyhow::bail!("No support for Instantiate"),
            WasmMsg::Migrate {
                contract_addr,
                new_code_id,
                msg,
            } => builder
                .add_migrate_message(
                    contract_addr.parse::<Address>()?,
                    sender,
                    *new_code_id,
                    convert_msg(msg)?,
                )
                .map(|_| ())
                .map_err(|e| e.into()),
            WasmMsg::UpdateAdmin { .. } => anyhow::bail!("No support for UpdateAdmin"),
            WasmMsg::ClearAdmin { contract_addr: _ } => anyhow::bail!("No support for ClearAdmin"),
            _ => anyhow::bail!("Unknown Wasm variant"),
        },
        _ => anyhow::bail!("Unknown CosmosMsg variant"),
    }
}

fn convert_msg(msg: &cosmwasm_std::Binary) -> Result<serde_json::Value> {
    serde_json::from_slice(msg.as_slice()).context("Unable to convert binary to JSON value")
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
