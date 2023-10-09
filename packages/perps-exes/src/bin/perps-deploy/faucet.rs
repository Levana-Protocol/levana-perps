use anyhow::Result;
use cosmos::{
    proto::cosmos::base::abci::v1beta1::TxResponse, Address, Contract, HasAddress, Wallet,
};
use msg::contracts::{
    cw20::Cw20Coin,
    faucet::entry::{
        ExecuteMsg, GetTokenResponse, IsAdminResponse, NextTradingIndexResponse, OwnerMsg, QueryMsg,
    },
};

#[derive(Clone)]
pub(crate) struct Faucet(Contract);

impl HasAddress for Faucet {
    fn get_address(&self) -> Address {
        self.0.get_address()
    }
}

impl Faucet {
    pub(crate) fn from_contract(contract: Contract) -> Self {
        Faucet(contract)
    }

    pub(crate) async fn get_cw20(
        &self,
        name: impl Into<String>,
        trading_competition_index: Option<u32>,
    ) -> Result<Option<Address>> {
        Ok(
            match self
                .0
                .query(QueryMsg::GetToken {
                    name: name.into(),
                    trading_competition_index,
                })
                .await?
            {
                GetTokenResponse::Found { address } => Some(address.into_string().parse()?),
                GetTokenResponse::NotFound {} => None,
            },
        )
    }

    pub(crate) async fn deploy_token(
        &self,
        wallet: &Wallet,
        name: impl Into<String>,
        trading_competition_index: Option<u32>,
    ) -> Result<TxResponse> {
        let name = name.into();
        let tap_amount = match name.as_str() {
            "ATOM" => "1000",
            "stATOM" => "1000",
            "USDC" => "20000",
            "BTC" => "1",
            "OSMO" => "2000",
            "SEI" => "2000",
            "ETH" => "2",
            "axlETH" => "2",
            "EVMOS" => "10000",
            "AKT" => "10000",
            "DOT" => "500",
            name => anyhow::bail!("Unknown collateral type: {name}"),
        }
        .parse()?;
        self.0
            .execute(
                wallet,
                vec![],
                ExecuteMsg::OwnerMsg(OwnerMsg::DeployToken {
                    name,
                    tap_amount,
                    trading_competition_index,
                    initial_balances: vec![],
                }),
            )
            .await
    }

    pub(crate) async fn next_trading_index(&self, name: impl Into<String>) -> Result<u32> {
        let NextTradingIndexResponse { next_index } = self
            .0
            .query(QueryMsg::NextTradingIndex { name: name.into() })
            .await?;
        Ok(next_index)
    }

    pub(crate) async fn mint(
        &self,
        wallet: &Wallet,
        cw20: impl HasAddress,
        balances: Vec<Cw20Coin>,
    ) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                ExecuteMsg::OwnerMsg(OwnerMsg::Mint {
                    cw20: cw20.get_address_string(),
                    balances,
                }),
            )
            .await
    }

    /// For trading competition only
    pub(crate) async fn set_market_address(
        &self,
        wallet: &Wallet,
        name: impl Into<String>,
        trading_competition_index: u32,
        market: impl HasAddress,
    ) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                ExecuteMsg::OwnerMsg(OwnerMsg::SetMarketAddress {
                    name: name.into(),
                    trading_competition_index,
                    market: market.get_address_string().into(),
                }),
            )
            .await
    }

    pub(crate) async fn is_admin(&self, new_admin: impl HasAddress) -> Result<bool> {
        let IsAdminResponse { is_admin } = self
            .0
            .query(QueryMsg::IsAdmin {
                addr: new_admin.get_address_string().into(),
            })
            .await?;
        Ok(is_admin)
    }

    pub(crate) async fn add_admin(
        &self,
        wallet: &Wallet,
        new_admin: impl HasAddress,
    ) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                ExecuteMsg::OwnerMsg(OwnerMsg::AddAdmin {
                    admin: new_admin.get_address_string().into(),
                }),
            )
            .await
    }
}
