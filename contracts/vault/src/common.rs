use cosmwasm_std::{QueryRequest, Storage, Uint64, WasmQuery};
use perpswap::contracts::vault::{Config, UsdcAsset};

use crate::{
    prelude::*,
    state::{self, QueueId},
    types::{
        MarketAllocation, MarketAllocationsResponse, TotalAssetsResponse, VaultBalanceResponse,
    },
};

pub fn get_total_assets(deps: Deps, env: &Env) -> Result<TotalAssetsResponse> {
    let config = state::CONFIG.load(deps.storage)?;

    let vault_balance = deps
        .querier
        .query_balance(&env.contract.address, config.usdc_denom.to_string())?
        .amount;

    let allocated_amount: Uint128 = state::MARKET_ALLOCATIONS
        .range(deps.storage, None, None, Order::Ascending)
        .try_fold(Uint128::zero(), |acc, item| -> Result<Uint128> {
            Ok(acc + item?.1)
        })?;

    let total_assets = vault_balance
        .checked_add(allocated_amount)
        .map_err(|_| anyhow!("Overflow in total assets calculation"))?;
    Ok(TotalAssetsResponse { total_assets })
}

pub fn get_vault_balance(deps: Deps, env: &Env) -> Result<VaultBalanceResponse> {
    let config = state::CONFIG.load(deps.storage)?;

    let vault_balance = match &config.usdc_denom {
        UsdcAsset::CW20(addr) => {
            let res: VaultBalanceResponse =
                deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                    contract_addr: addr.to_string(),
                    msg: to_json_binary(&cw20::Cw20QueryMsg::Balance {
                        address: env.contract.address.to_string(),
                    })?,
                }))?;
            res.vault_balance
        }
        UsdcAsset::Native(denom) => {
            deps.querier
                .query_balance(&env.contract.address, denom)?
                .amount
        }
    };

    let allocated_amount = state::MARKET_ALLOCATIONS
        .range(deps.storage, None, None, Order::Ascending)
        .try_fold(Uint128::zero(), |acc, res| -> Result<Uint128> {
            Ok(acc + res?.1)
        })?;

    let pending_withdrawals = state::TOTAL_PENDING_WITHDRAWALS
        .load(deps.storage)
        .unwrap_or(Uint128::zero());

    let total_allocated = vault_balance
        .checked_add(allocated_amount)
        .map_err(|_| anyhow!("Overflow in total calculation"))?;

    Ok(VaultBalanceResponse {
        vault_balance,
        allocated_amount,
        pending_withdrawals,
        total_allocated,
    })
}

pub fn get_market_allocations(
    deps: Deps,
    start_after: Option<String>,
) -> Result<MarketAllocationsResponse> {
    let start: Option<Bound<&str>> = start_after.as_deref().map(Bound::exclusive);

    let allocations: Vec<MarketAllocation> = state::MARKET_ALLOCATIONS
        .range(deps.storage, start, None, Order::Ascending)
        .map(|item| {
            let (market_id, amount) = item?;
            Ok(MarketAllocation {
                market_id: market_id.to_string(),
                amount,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(MarketAllocationsResponse { allocations })
}

pub fn check_not_paused(config: &Config) -> Result<()> {
    if config.paused {
        return Err(anyhow!("Contract operations are paused"));
    }
    Ok(())
}

pub fn get_and_increment_queue_id(store: &mut dyn Storage) -> Result<QueueId> {
    let current_id = state::QUEUE_ID
        .may_load(store)?
        .unwrap_or(QueueId(Uint64::zero()));
    let next_id = QueueId(current_id.0 + Uint64::one());
    state::QUEUE_ID.save(store, &next_id)?;
    Ok(current_id)
}
