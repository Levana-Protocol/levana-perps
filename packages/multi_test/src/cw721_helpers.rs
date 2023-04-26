use anyhow::Result;
use cosmwasm_std::Addr;
use cw_multi_test::{AppResponse, Executor};
use msg::contracts::position_token::{
    entry::{ExecuteMsg as Cw721ExecuteMsg, QueryMsg as Cw721QueryMsg},
    Metadata,
};
use serde::de::DeserializeOwned;

use super::PerpsApp;

impl PerpsApp {
    pub(crate) fn cw721_exec(
        &mut self,
        sender: Addr,
        addr: Addr,
        msg: &Cw721ExecuteMsg,
    ) -> Result<AppResponse> {
        self.execute_contract(sender, addr, msg, &[])
    }

    pub(crate) fn cw721_query<T: DeserializeOwned>(
        &mut self,
        addr: Addr,
        msg: &Cw721QueryMsg,
    ) -> Result<T> {
        self.app
            .wrap()
            .query_wasm_smart(addr, &msg)
            .map_err(|err| err.into())
    }
}

pub trait NftMetadataExt {
    fn get_attr(&self, key: &str) -> Option<&str>;
}

impl NftMetadataExt for Metadata {
    fn get_attr(&self, key: &str) -> Option<&str> {
        match &self.attributes {
            None => None,
            Some(attributes) => attributes.iter().find_map(|t| {
                if t.trait_type == key {
                    Some(t.value.as_str())
                } else {
                    None
                }
            }),
        }
    }
}
