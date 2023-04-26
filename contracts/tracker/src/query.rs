use anyhow::Result;
use cosmwasm_std::{entry_point, to_binary, Addr, Storage};
use cosmwasm_std::{Binary, Deps, Env};
use msg::contracts::tracker::entry::{CodeIdResp, ContractResp, QueryMsg};

use crate::state::{CODE_BY_HASH, CODE_BY_ID, CONTRACT_BY_ADDR, CONTRACT_BY_FAMILY};

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary> {
    Ok(match msg {
        QueryMsg::CodeById { code_id } => code_id_resp(deps.storage, Some(code_id))?,
        QueryMsg::CodeByHash { hash } => {
            let code_id = CODE_BY_HASH.may_load(deps.storage, &hash)?;
            code_id_resp(deps.storage, code_id)?
        }
        QueryMsg::ContractByAddress { address } => {
            let address = deps.api.addr_validate(&address)?;
            contract_resp(deps.storage, Some(address))?
        }
        QueryMsg::ContractByFamily {
            contract_type,
            family,
            sequence,
        } => {
            let address = match sequence {
                Some(sequence) => CONTRACT_BY_FAMILY
                    .may_load(deps.storage, ((&family, &contract_type), sequence))?,
                None => CONTRACT_BY_FAMILY
                    .prefix((&family, &contract_type))
                    .range(deps.storage, None, None, cosmwasm_std::Order::Descending)
                    .next()
                    .transpose()?
                    .map(|x| x.1),
            };
            contract_resp(deps.storage, address)?
        }
    })
}

fn code_id_resp(store: &dyn Storage, code_id: Option<u64>) -> Result<Binary> {
    to_binary(&match code_id {
        None => CodeIdResp::NotFound {},
        Some(code_id) => match CODE_BY_ID.may_load(store, code_id)? {
            None => CodeIdResp::NotFound {},
            Some(info) => CodeIdResp::Found {
                contract_type: info.contract_type,
                code_id: info.code_id,
                hash: info.hash,
                tracked_at: info.tracked_at,
                gitrev: info.gitrev,
            },
        },
    })
    .map_err(|e| e.into())
}

fn contract_resp(store: &dyn Storage, address: Option<Addr>) -> Result<Binary> {
    to_binary(&match address {
        None => ContractResp::NotFound {},
        Some(address) => match CONTRACT_BY_ADDR.may_load(store, &address)? {
            None => ContractResp::NotFound {},
            Some(info) => {
                let code_id_info = CODE_BY_ID.load(store, info.current_code_id)?;
                ContractResp::Found {
                    address: address.into_string(),
                    contract_type: code_id_info.contract_type,
                    original_code_id: info.original_code_id,
                    original_tracked_at: info.original_tracked_at,
                    current_code_id: info.current_code_id,
                    current_tracked_at: info.current_tracked_at,
                    family: info.family,
                    sequence: info.sequence,
                    migrate_count: info.migrate_count,
                }
            }
        },
    })
    .map_err(|e| e.into())
}
