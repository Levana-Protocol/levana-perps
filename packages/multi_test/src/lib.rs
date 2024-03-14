#![deny(clippy::as_conversions)]

pub mod config;
pub mod cw20_helpers;
pub mod cw721_helpers;
pub mod extensions;
pub mod macros;
pub mod market_wrapper;
pub mod market_wrapper_scenarios;
pub mod position_helpers;
pub mod response;
//pub mod test_strategies;
pub mod arbitrary;
pub mod contracts;
pub mod rewards_helpers;
pub mod simple_oracle;
pub mod time;

use anyhow::{anyhow, bail, Context, Result};
use config::TEST_CONFIG;
use cosmwasm_std::{
    from_slice, Addr, Binary, Deps, DepsMut, Empty, Env, MessageInfo, QuerierWrapper,
    QueryResponse, Reply, Response,
};
use cw_multi_test::{App, AppResponse, BankSudo, Contract, Executor, SudoMsg};
use dotenv::dotenv;
use msg::contracts::rewards::entry::ConfigUpdate;
use msg::prelude::*;
use msg::token::Token;
use num_traits::ToPrimitive;
use rand::rngs::ThreadRng;
use serde::{de::DeserializeOwned, Serialize};
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::{fmt::Debug, marker::PhantomData};
use time::BlockInfoChange;

/**
 * Base app for mocking perps
 */
pub struct PerpsApp {
    code_ids: HashMap<PerpsContract, u64>,
    cw20_addrs: HashMap<String, Addr>,
    app: App,
    pub rng: ThreadRng,
    pub users: HashSet<Addr>,
    pub factory_addr: Addr,
    pub log_block_time_changes: bool,
    pub rewards_addr: Addr,
    pub simple_oracle_addr: Addr,
    pub simple_oracle_usd_addr: Addr,
}

impl Deref for PerpsApp {
    type Target = App;
    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

impl DerefMut for PerpsApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}

/**
 * Identifies a perps contract
 */
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PerpsContract {
    Factory,
    Market,
    PositionToken,
    LiquidityToken,
    Cw20,
    Rewards,
    Farming,
    SimpleOracle,
}

impl PerpsApp {
    pub fn new_cell() -> Result<Rc<RefCell<Self>>> {
        Ok(Rc::new(RefCell::new(Self::new()?)))
    }

    pub(crate) fn new() -> Result<Self> {
        dotenv().ok();
        let mut app = App::default();

        let factory_code_id = app.store_code(contract_factory());
        let market_code_id = app.store_code(contract_market());
        let cw20_code_id = app.store_code(contract_cw20());
        let position_token_code_id = app.store_code(contract_position_token());
        let liquidity_token_code_id = app.store_code(contract_liquidity_token());
        let rewards_code_id = app.store_code(contract_rewards());
        let farming_code_id = app.store_code(contract_farming());
        let simple_oracle_code_id = app.store_code(contract_simple_oracle());

        let factory_addr = app.instantiate_contract(
            factory_code_id,
            Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &msg::contracts::factory::entry::InstantiateMsg {
                market_code_id: market_code_id.to_string(),
                position_token_code_id: position_token_code_id.to_string(),
                liquidity_token_code_id: liquidity_token_code_id.to_string(),
                migration_admin: TEST_CONFIG.migration_admin.clone().into(),
                owner: TEST_CONFIG.protocol_owner.clone().into(),
                dao: TEST_CONFIG.dao.clone().into(),
                kill_switch: TEST_CONFIG.kill_switch.clone().into(),
                wind_down: TEST_CONFIG.wind_down.clone().into(),
                label_suffix: Some(" - MULTITEST".to_owned()),
            },
            &[],
            "factory",
            Some(TEST_CONFIG.migration_admin.clone()),
        )?;

        let rewards_addr = app.instantiate_contract(
            rewards_code_id,
            Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &msg::contracts::rewards::entry::InstantiateMsg {
                config: ConfigUpdate {
                    immediately_transferable: Decimal256::from_str("0.25")?,
                    token_denom: TEST_CONFIG.rewards_token_denom.clone(),
                    unlock_duration_seconds: 60,
                    factory_addr: factory_addr.clone().into_string(),
                },
            },
            &[],
            "rewards",
            Some(TEST_CONFIG.migration_admin.clone()),
        )?;

        let simple_oracle_addr = app.instantiate_contract(
            simple_oracle_code_id,
            Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &simple_oracle::InstantiateMsg {
                owner: TEST_CONFIG.protocol_owner.clone().into(),
            },
            &[],
            "rewards",
            Some(TEST_CONFIG.migration_admin.clone()),
        )?;
        let simple_oracle_usd_addr = app.instantiate_contract(
            simple_oracle_code_id,
            Addr::unchecked(&TEST_CONFIG.protocol_owner),
            &simple_oracle::InstantiateMsg {
                owner: TEST_CONFIG.protocol_owner.clone().into(),
            },
            &[],
            "rewards",
            Some(TEST_CONFIG.migration_admin.clone()),
        )?;

        let mut _self = PerpsApp {
            code_ids: [
                (PerpsContract::Factory, factory_code_id),
                (PerpsContract::Market, market_code_id),
                (PerpsContract::Cw20, cw20_code_id),
                (PerpsContract::PositionToken, position_token_code_id),
                (PerpsContract::LiquidityToken, liquidity_token_code_id),
                (PerpsContract::Rewards, rewards_code_id),
                (PerpsContract::Farming, farming_code_id),
                (PerpsContract::Farming, farming_code_id),
                (PerpsContract::SimpleOracle, simple_oracle_code_id),
            ]
            .into(),
            app,
            factory_addr,
            cw20_addrs: HashMap::new(),
            rng: rand::thread_rng(),
            users: HashSet::new(),
            log_block_time_changes: false,
            rewards_addr,
            simple_oracle_addr,
            simple_oracle_usd_addr,
        };

        Ok(_self)
    }

    // returned bool is true iff it's a newly created user
    pub fn get_user(&mut self, name: &str, token: &Token, funds: Number) -> Result<(Addr, bool)> {
        let addr = Addr::unchecked(name);
        if self.users.contains(&addr) {
            Ok((addr, false))
        } else {
            self.mint_token(
                &addr,
                token,
                NonZero::try_from_number(funds).context("Need non-zero tokens")?,
            )?;
            self.users.insert(addr.clone());
            Ok((addr, true))
        }
    }

    /**
     * Mint coins to an account
     *
     * * `addr` - Receiver of minted coins
     * * `coins` - Coins to mint
     */
    pub fn mint_token(
        &mut self,
        recipient: &Addr,
        token: &Token,
        amount: NonZero<Collateral>,
    ) -> Result<AppResponse> {
        match token {
            Token::Cw20 { addr, .. } => self.cw20_mint(
                &Addr::unchecked(addr.clone().into_string()),
                &Addr::unchecked(&TEST_CONFIG.protocol_owner),
                recipient,
                token
                    .into_u128(amount.into_decimal256())?
                    .ok_or_else(|| anyhow!("no amount!"))?
                    .into(),
            ),
            Token::Native { .. } => self.app.sudo(SudoMsg::Bank(BankSudo::Mint {
                to_address: recipient.to_string(),
                amount: vec![token
                    .into_native_coin(amount.into_number_gt_zero())?
                    .ok_or_else(|| anyhow!("no coin!"))?],
            })),
        }
    }

    /**
     * Fetch the code ID of an perps contract
     *
     * * `contract` - Uploaded contract name
     */
    pub(crate) fn code_id(&self, contract: PerpsContract) -> Result<u64> {
        self.code_ids.get(&contract).copied().context("no code id")
    }

    /**
     * Return an object that allows querying of current blockchain state
     */
    pub fn querier(&self) -> QuerierWrapper {
        self.app.wrap()
    }

    pub fn set_block_info(&mut self, change: BlockInfoChange) {
        self.app.update_block(|block_info| {
            let BlockInfoChange { height, nanos } = change;
            let time_before = block_info.time;
            let height_before = block_info.height;
            if nanos < 0 {
                block_info.time = block_info.time.minus_nanos(nanos.unsigned_abs());
            } else {
                block_info.time = block_info.time.plus_nanos(nanos.to_u64().unwrap_or_default());
            }
            if height < 0 {
                block_info.height -= height.unsigned_abs();
            } else {
                block_info.height += height.to_u64().unwrap_or_default();
            }
            if self.log_block_time_changes {
                println!(
                    "moving forward {} blocks ({} -> {}) and {} nanoseconds ({} -> {})",
                    height, height_before, block_info.height, nanos, time_before, block_info.time
                );
            }
        });
    }

    pub fn get_cw20_addr(&mut self, symbol: impl Into<String>) -> Result<Addr> {
        let symbol: String = symbol.into();
        let code_id = self.code_id(PerpsContract::Cw20)?;

        match self.cw20_addrs.entry(symbol.clone()) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                let msg = msg::contracts::cw20::entry::InstantiateMsg {
                    name: symbol.clone(),
                    symbol,
                    decimals: TEST_CONFIG.cw20_decimals,
                    initial_balances: Vec::new(),
                    minter: msg::contracts::cw20::entry::InstantiateMinter {
                        minter: TEST_CONFIG.protocol_owner.clone().into(),
                        cap: None,
                    },
                    marketing: None,
                };

                let addr = Addr::unchecked(
                    self.app
                        .instantiate_contract(
                            code_id,
                            Addr::unchecked(&TEST_CONFIG.protocol_owner),
                            &msg,
                            &[],
                            "mock cw20",
                            Some(TEST_CONFIG.migration_admin.clone()),
                        )
                        .unwrap(),
                );

                entry.insert(addr.clone());

                Ok(addr)
            }
        }
    }
}

pub(crate) fn contract_position_token() -> Box<dyn Contract<Empty>> {
    Box::new(LocalContractWrapper::new(
        position_token::contract::instantiate,
        position_token::contract::execute,
        position_token::contract::query,
    ))
}

pub(crate) fn contract_liquidity_token() -> Box<dyn Contract<Empty>> {
    Box::new(LocalContractWrapper::new(
        liquidity_token::contract::instantiate,
        liquidity_token::contract::execute,
        liquidity_token::contract::query,
    ))
}

pub(crate) fn contract_cw20() -> Box<dyn Contract<Empty>> {
    Box::new(LocalContractWrapper::new(
        cw20::contract::instantiate,
        cw20::contract::execute,
        cw20::contract::query,
    ))
}

pub(crate) fn contract_market() -> Box<dyn Contract<Empty>> {
    Box::new(
        LocalContractWrapper::new(
            market::contract::instantiate,
            market::contract::execute,
            market::contract::query,
        )
        .with_reply(market::contract::reply),
    )
}

pub(crate) fn contract_factory() -> Box<dyn Contract<Empty>> {
    Box::new(
        LocalContractWrapper::new(
            factory::contract::instantiate,
            factory::contract::execute,
            factory::contract::query,
        )
        .with_reply(factory::contract::reply),
    )
}

pub(crate) fn contract_rewards() -> Box<dyn Contract<Empty>> {
    Box::new(LocalContractWrapper::new(
        rewards::contract::instantiate,
        rewards::contract::execute,
        rewards::contract::query,
    ))
}

pub(crate) fn contract_farming() -> Box<dyn Contract<Empty>> {
    Box::new(
        LocalContractWrapper::new(
            farming::lifecycle::instantiate,
            farming::execute::execute,
            farming::query::query,
        )
        .with_reply(farming::execute::reply),
    )
}

pub(crate) fn contract_simple_oracle() -> Box<dyn Contract<Empty>> {
    Box::new(LocalContractWrapper::new(
        simple_oracle::instantiate,
        simple_oracle::execute,
        simple_oracle::query,
    ))
}

// struct to satisfy the `Contract` trait
pub(crate) struct LocalContractWrapper<
    Instantiate,
    InstantiateMsg,
    Execute,
    ExecuteMsg,
    Query,
    QueryMsg,
> where
    Instantiate: Fn(DepsMut, Env, MessageInfo, InstantiateMsg) -> Result<Response> + 'static,
    Execute: Fn(DepsMut, Env, MessageInfo, ExecuteMsg) -> Result<Response> + 'static,
    Query: Fn(Deps, Env, QueryMsg) -> Result<QueryResponse> + 'static,
    InstantiateMsg: Serialize + DeserializeOwned + Debug + 'static,
    ExecuteMsg: Serialize + DeserializeOwned + Debug + 'static,
    QueryMsg: Serialize + DeserializeOwned + 'static,
{
    instantiate: Instantiate,
    execute: Execute,
    query: Query,
    reply: Option<ReplyFn>,
    instantiate_msg: PhantomData<InstantiateMsg>,
    execute_msg: PhantomData<ExecuteMsg>,
    query_msg: PhantomData<QueryMsg>,
}

type ReplyFn = fn(DepsMut, Env, Reply) -> Result<Response>;

impl<Instantiate, InstantiateMsg, Execute, ExecuteMsg, Query, QueryMsg>
    LocalContractWrapper<Instantiate, InstantiateMsg, Execute, ExecuteMsg, Query, QueryMsg>
where
    Instantiate: Fn(DepsMut, Env, MessageInfo, InstantiateMsg) -> Result<Response> + 'static,
    Execute: Fn(DepsMut, Env, MessageInfo, ExecuteMsg) -> Result<Response> + 'static,
    Query: Fn(Deps, Env, QueryMsg) -> Result<QueryResponse> + 'static,
    InstantiateMsg: Serialize + DeserializeOwned + Debug + 'static,
    ExecuteMsg: Serialize + DeserializeOwned + Debug + 'static,
    QueryMsg: Serialize + DeserializeOwned + 'static,
{
    pub fn new(instantiate: Instantiate, execute: Execute, query: Query) -> Self {
        Self {
            instantiate,
            execute,
            query,
            reply: None,
            instantiate_msg: PhantomData,
            execute_msg: PhantomData,
            query_msg: PhantomData,
        }
    }

    pub(crate) fn with_reply(
        self,
        reply_fn: fn(DepsMut, Env, Reply) -> Result<Response<Empty>>,
    ) -> Self {
        LocalContractWrapper {
            instantiate: self.instantiate,
            execute: self.execute,
            query: self.query,
            reply: Some(reply_fn),
            instantiate_msg: self.instantiate_msg,
            execute_msg: self.execute_msg,
            query_msg: self.query_msg,
        }
    }
}

impl<Instantiate, InstantiateMsg, Execute, ExecuteMsg, Query, QueryMsg> Contract<Empty, Empty>
    for LocalContractWrapper<Instantiate, InstantiateMsg, Execute, ExecuteMsg, Query, QueryMsg>
where
    Instantiate: Fn(DepsMut, Env, MessageInfo, InstantiateMsg) -> Result<Response> + 'static,
    Execute: Fn(DepsMut, Env, MessageInfo, ExecuteMsg) -> Result<Response> + 'static,
    Query: Fn(Deps, Env, QueryMsg) -> Result<QueryResponse> + 'static,
    InstantiateMsg: Serialize + DeserializeOwned + Debug + 'static,
    ExecuteMsg: Serialize + DeserializeOwned + Debug + 'static,
    QueryMsg: Serialize + DeserializeOwned + 'static,
{
    fn execute(
        &self,
        deps: DepsMut<Empty>,
        env: Env,
        info: MessageInfo,
        msg: Vec<u8>,
    ) -> Result<Response<Empty>> {
        let msg: ExecuteMsg = from_slice(&msg)?;
        (self.execute)(deps, env, info, msg)
    }

    fn instantiate(
        &self,
        deps: DepsMut<Empty>,
        env: Env,
        info: MessageInfo,
        msg: Vec<u8>,
    ) -> Result<Response<Empty>> {
        let msg: InstantiateMsg = from_slice(&msg)?;
        (self.instantiate)(deps, env, info, msg)
    }

    fn query(&self, deps: Deps<Empty>, env: Env, msg: Vec<u8>) -> Result<Binary> {
        let msg: QueryMsg = from_slice(&msg)?;
        (self.query)(deps, env, msg)
    }

    fn sudo(&self, _deps: DepsMut<Empty>, _env: Env, _msg: Vec<u8>) -> Result<Response<Empty>> {
        bail!("sudo not implemented for contract")
    }

    // this returns an error if the contract doesn't implement reply
    fn reply(&self, deps: DepsMut<Empty>, env: Env, reply_data: Reply) -> Result<Response<Empty>> {
        match self.reply {
            Some(reply) => (reply)(deps, env, reply_data),
            None => bail!("reply not implemented for contract"),
        }
    }

    // this returns an error if the contract doesn't implement migrate
    fn migrate(&self, _deps: DepsMut<Empty>, _env: Env, _msg: Vec<u8>) -> Result<Response<Empty>> {
        bail!("migrate not implemented for contract")
    }
}
