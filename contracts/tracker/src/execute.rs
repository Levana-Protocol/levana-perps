use anyhow::{Context, Result};
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};
use msg::contracts::tracker::entry::ExecuteMsg;
use msg::contracts::tracker::events::{InstantiateEvent, MigrateEvent, NewCodeIdEvent};

use crate::state::{
    CodeIdInfo, ContractInfo, ADMINS, CODE_BY_HASH, CODE_BY_ID, CONTRACT_BY_ADDR,
    CONTRACT_BY_FAMILY,
};

#[entry_point]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    anyhow::ensure!(
        ADMINS.has(deps.storage, &info.sender)
            || deps
                .querier
                .query_wasm_contract_info(env.contract.address)?
                .admin
                .as_deref()
                == Some(info.sender.as_str()),
        "Not in the admin list"
    );
    Ok(match msg {
        ExecuteMsg::CodeId {
            contract_type,
            code_id,
            hash,
            gitrev,
        } => {
            anyhow::ensure!(
                !CODE_BY_ID.has(deps.storage, code_id),
                "Code ID already submitted"
            );
            anyhow::ensure!(
                !CODE_BY_HASH.has(deps.storage, &hash),
                "Code hash already submitted"
            );
            CODE_BY_HASH.save(deps.storage, &hash, &code_id)?;
            let info = CodeIdInfo {
                contract_type,
                code_id,
                hash,
                tracked_at: env.block.time.into(),
                gitrev,
            };
            CODE_BY_ID.save(deps.storage, code_id, &info)?;
            Response::new().add_event(
                NewCodeIdEvent {
                    contract_type: info.contract_type,
                    code_id,
                    hash: info.hash,
                }
                .into(),
            )
        }
        ExecuteMsg::Instantiate {
            code_id,
            address,
            family,
        } => {
            let address = deps.api.addr_validate(&address)?;
            let code_id_info = CODE_BY_ID
                .may_load(deps.storage, code_id)?
                .context("Specified code ID not found")?;

            let new_sequence = CONTRACT_BY_FAMILY
                .prefix((&family, &code_id_info.contract_type))
                .keys(deps.storage, None, None, cosmwasm_std::Order::Descending)
                .next()
                .transpose()?
                .map_or(0, |x| x + 1);
            CONTRACT_BY_FAMILY.save(
                deps.storage,
                ((&family, &code_id_info.contract_type), new_sequence),
                &address,
            )?;

            let info = ContractInfo {
                original_code_id: code_id,
                original_tracked_at: env.block.time.into(),
                current_code_id: code_id,
                current_tracked_at: env.block.time.into(),
                family,
                sequence: new_sequence,
                migrate_count: 0,
            };
            CONTRACT_BY_ADDR.save(deps.storage, &address, &info)?;

            Response::new().add_event(
                InstantiateEvent {
                    contract_type: code_id_info.contract_type,
                    code_id,
                    hash: code_id_info.hash,
                    address: address.into_string(),
                    family: info.family,
                    sequence: new_sequence,
                }
                .into(),
            )
        }
        ExecuteMsg::Migrate {
            new_code_id,
            address,
        } => {
            let address = deps.api.addr_validate(&address)?;
            let new_code_info = CODE_BY_ID
                .may_load(deps.storage, new_code_id)?
                .context("New code ID not found")?;
            let mut contract_info = CONTRACT_BY_ADDR
                .may_load(deps.storage, &address)?
                .context("Contract not found")?;
            let old_code_id = contract_info.current_code_id;
            let old_code_info = CODE_BY_ID
                .may_load(deps.storage, old_code_id)?
                .context("Old code ID not found")?;
            anyhow::ensure!(
                old_code_info.contract_type == new_code_info.contract_type,
                "Attempting to migrate from {} to {}",
                old_code_info.contract_type,
                new_code_info.contract_type
            );
            contract_info.current_code_id = new_code_id;
            contract_info.current_tracked_at = env.block.time.into();
            contract_info.migrate_count += 1;
            CONTRACT_BY_ADDR.save(deps.storage, &address, &contract_info)?;

            Response::new().add_event(
                MigrateEvent {
                    contract_type: old_code_info.contract_type,
                    old_code_id,
                    new_code_id,
                    old_hash: old_code_info.hash,
                    new_hash: new_code_info.hash,
                    address: address.into_string(),
                    family: contract_info.family,
                    sequence: contract_info.sequence,
                    new_migrate_count: contract_info.migrate_count,
                }
                .into(),
            )
        }
        ExecuteMsg::AddAdmin { address } => {
            let address = deps.api.addr_validate(&address)?;
            anyhow::ensure!(!ADMINS.has(deps.storage, &address), "Already an admin");
            ADMINS.save(deps.storage, &address, &())?;
            Response::new()
        }
        ExecuteMsg::RemoveAdmin { address } => {
            let address = deps.api.addr_validate(&address)?;
            anyhow::ensure!(ADMINS.has(deps.storage, &address), "Not an admin");
            ADMINS.remove(deps.storage, &address);
            Response::new()
        }
    })
}
