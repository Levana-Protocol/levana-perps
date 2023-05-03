use crate::{PerpsApp, TEST_CONFIG};
use anyhow::Result;
use cosmwasm_std::{Addr, Decimal256};
use cw_multi_test::{AppResponse, Executor};
use msg::contracts::rewards::config::Config;
use msg::contracts::rewards::entry::ExecuteMsg::{
    Claim, ConfigUpdate as ConfigUpdateMsg, DistributeRewards,
};
use msg::contracts::rewards::entry::QueryMsg::{Config as ConfigQuery, RewardsInfo};
use msg::contracts::rewards::entry::{ConfigUpdate, RewardsInfoResp};
use msg::prelude::RawAddr;
use msg::token::Token;

impl PerpsApp {
    pub(crate) fn rewards_token(&self) -> Token {
        Token::Native {
            denom: TEST_CONFIG.rewards_token_denom.clone(),
            decimal_places: 6,
        }
    }

    pub fn setup_rewards_contract(&mut self) {
        let rewards_addr = self.rewards_addr.clone();
        let factory_addr = self.factory_addr.clone().into_string();

        self.execute_contract(
            Addr::unchecked(&TEST_CONFIG.protocol_owner),
            rewards_addr.clone(),
            &ConfigUpdateMsg {
                config: ConfigUpdate {
                    immediately_transferable: "0.25".parse().unwrap(),
                    token_denom: TEST_CONFIG.rewards_token_denom.clone(),
                    unlock_duration_seconds: 60,
                    factory_addr,
                },
            },
            &[],
        )
        .unwrap();

        self.mint_token(
            &rewards_addr,
            &self.rewards_token(),
            "10000".parse().unwrap(),
        )
        .unwrap();
    }

    pub fn distribute_rewards(
        &mut self,
        recipient: impl Into<RawAddr>,
        amount: &str,
    ) -> Result<AppResponse> {
        let rewards_addr = self.rewards_addr.clone();

        self.execute_contract(
            Addr::unchecked("sender"),
            rewards_addr,
            &DistributeRewards {
                address: recipient.into(),
                amount: amount.parse().unwrap(),
            },
            &[],
        )
    }

    pub fn claim_rewards(&mut self, recipient: &Addr) -> Result<AppResponse> {
        let rewards_addr = self.rewards_addr.clone();
        self.execute_contract(recipient.clone(), rewards_addr, &Claim {}, &[])
    }

    pub fn query_rewards_info(&self, addr: impl Into<RawAddr>) -> Result<Option<RewardsInfoResp>> {
        self.wrap()
            .query_wasm_smart(
                self.rewards_addr.clone(),
                &RewardsInfo { addr: addr.into() },
            )
            .map_err(|e| e.into())
    }

    pub fn query_rewards_balance(&self, addr: &Addr) -> Result<Decimal256> {
        let amount = self
            .wrap()
            .query_balance(addr, TEST_CONFIG.rewards_token_denom.clone())?
            .amount
            .u128();
        self.rewards_token().from_u128(amount)
    }

    pub fn query_rewards_config(&self) -> Result<Config> {
        self.wrap()
            .query_wasm_smart(self.rewards_addr.clone(), &ConfigQuery {})
            .map_err(|e| e.into())
    }

    pub fn update_rewards_config(
        &mut self,
        sender: Addr,
        config: ConfigUpdate,
    ) -> Result<AppResponse> {
        let rewards_addr = self.rewards_addr.clone();
        self.execute_contract(sender, rewards_addr, &ConfigUpdateMsg { config }, &[])
    }
}
