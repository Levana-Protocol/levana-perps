// This is essentially a local clone of https://github.com/Levana-Protocol/simple-oracle
// but for testing
// For the sake of reducing clutter, it's also flattened into one file here

use anyhow::{bail, Result};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Deps, DepsMut, Env, Event, MessageInfo, QueryResponse, Response,
};
use cosmwasm_std::{Addr, BlockInfo, Decimal256, Timestamp};
use cw2::set_contract_version;
use cw_storage_plus::Item;

pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const OWNER: Item<Addr> = Item::new("owner");
pub const PRICE: Item<Price> = Item::new("price");

#[cw_serde]
pub struct InstantiateMsg {
    /// the owner of the contract who can execute value changes
    /// if not set, then it will be the instantiator
    pub owner: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Change the owner
    SetOwner {
        /// The owner address
        owner: String,
    },

    /// Set the price
    SetPrice {
        /// The new price value
        value: Decimal256,
        /// Optional timestamp for the price, independent of block time
        timestamp: Option<Timestamp>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Get the current price
    #[returns(Price)]
    Price {},
    /// Get the owner
    #[returns(OwnerResp)]
    Owner {},
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct OwnerResp {
    /// The owner address
    pub owner: Addr,
}

#[cw_serde]
pub struct Price {
    /// The price value set via `ExecuteMsg::SetPrice`
    pub value: Decimal256,
    /// The block info when this price was set
    pub block_info: BlockInfo,
    /// Optional timestamp for the price, independent of block_info.time
    pub timestamp: Option<Timestamp>,
    /// FIXME: Currently required by market contract... might be changed soon
    pub volatile: bool,
}

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let owner = msg
        .owner
        .as_ref()
        .map(|owner| deps.api.addr_validate(owner))
        .transpose()?
        .unwrap_or(info.sender);

    OWNER
        .save(deps.storage, &owner)
        .map_err(anyhow::Error::from)?;

    Ok(
        Response::new().add_event(Event::new("instantiation").add_attributes([
            ("owner", owner.as_str()),
            ("contract_name", CONTRACT_NAME),
            ("contract_version", CONTRACT_VERSION),
        ])),
    )
}

pub fn sudo(_deps: DepsMut, _env: Env, _msg: ExecuteMsg) -> Result<Response> {
    todo!()
}

pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
    // all execution messages require the sender to be the owner
    let owner = OWNER.load(deps.storage)?;
    if info.sender != owner {
        bail!(
            "unauthorized, owner is {} (msg sent from {}",
            owner,
            info.sender
        );
    }

    match msg {
        ExecuteMsg::SetOwner { owner } => {
            let owner = deps.api.addr_validate(&owner)?;
            OWNER.save(deps.storage, &owner)?;
            Ok(Response::new()
                .add_event(Event::new("set-owner").add_attribute("owner", owner.as_str())))
        }
        ExecuteMsg::SetPrice { value, timestamp } => {
            let price = Price {
                value,
                block_info: env.block,
                timestamp,
                volatile: true, // REQUIRED for market to see the publish_time
            };

            PRICE.save(deps.storage, &price)?;

            let mut event = Event::new("set-price").add_attribute("value", price.value.to_string());
            if let Some(timestamp) = timestamp {
                event = event.add_attribute("timestamp", timestamp.to_string());
            }

            Ok(Response::new().add_event(event))
        }
    }
}

pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<QueryResponse> {
    match msg {
        QueryMsg::Owner {} => {
            let owner = OWNER.load(deps.storage)?;
            let res = to_json_binary(&OwnerResp { owner })?;
            Ok(res)
        }
        QueryMsg::Price {} => {
            let price = PRICE.load(deps.storage)?;
            let res = to_json_binary(&price)?;
            Ok(res)
        }
    }
}
