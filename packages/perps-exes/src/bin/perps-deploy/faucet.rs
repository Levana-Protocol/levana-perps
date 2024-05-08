use anyhow::Result;
use cosmos::{
    proto::cosmos::base::abci::v1beta1::TxResponse, Address, Contract, HasAddress, HasAddressHrp,
    HasContract, HasCosmos, Wallet,
};
use cosmwasm_std::Decimal256;
use msg::contracts::{
    cw20::Cw20Coin,
    faucet::entry::{
        ExecuteMsg, GetTokenResponse, IsAdminResponse, NextTradingIndexResponse, OwnerMsg,
        QueryMsg, TapAmountResponse,
    },
};
use shared::storage::UnsignedDecimal;

#[derive(Clone)]
pub(crate) struct Faucet(Contract);

impl HasCosmos for Faucet {
    fn get_cosmos(&self) -> &cosmos::Cosmos {
        self.0.get_cosmos()
    }
}
impl HasContract for Faucet {
    fn get_contract(&self) -> &Contract {
        &self.0
    }
}
impl HasAddressHrp for Faucet {
    fn get_address_hrp(&self) -> cosmos::AddressHrp {
        self.0.get_address_hrp()
    }
}
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
    ) -> Result<()> {
        let name = name.into();
        let tap_amount: Decimal256 = match name.as_str() {
            "ATOM" | "amATOM" => "1000",
            "stATOM" => "1000",
            "USDC" => "20000",
            "USDT" => "20000",
            "BTC" | "wBTC" => "1",
            "OSMO" => "2000",
            "stOSMO" => "2000",
            "SEI" => "2000",
            "ETH" => "2",
            "axlETH" => "2",
            "EVMOS" => "10000",
            "AKT" => "10000",
            "DOT" => "500",
            "AXL" => "2000",
            "ryETH" => "2",
            "INJ" => "1000",
            "TIA" => "2000",
            "milkTIA" => "2000",
            "stDYDX" => "1000",
            "stTIA" => "2000",
            "DYM" => "2000",
            "stDYM" => "2000",
            "NTRN" => "2000",
            "SCRT" => "2000",
            name => anyhow::bail!("Unknown collateral type: {name}"),
        }
        .parse()?;
        let txres = self
            .0
            .execute(
                wallet,
                vec![],
                ExecuteMsg::OwnerMsg(OwnerMsg::DeployToken {
                    name: name.clone(),
                    tap_amount: tap_amount.into_signed(),
                    trading_competition_index,
                    initial_balances: vec![],
                }),
            )
            .await?;
        log::info!("Deployed new token in {}", txres.txhash);
        let tap_amount_resp: TapAmountResponse = self
            .0
            .query(QueryMsg::TapAmountByName { name: name.clone() })
            .await?;
        match tap_amount_resp {
            TapAmountResponse::CannotTap {} => {
                log::info!("No tap amount set in contract for {name}, adding.");
                let txres = self
                    .0
                    .execute(
                        wallet,
                        vec![],
                        ExecuteMsg::OwnerMsg(OwnerMsg::SetMultitapAmount {
                            name,
                            amount: tap_amount,
                        }),
                    )
                    .await?;
                log::info!("Tap amount set in {}", txres.txhash);
            }
            TapAmountResponse::CanTap { amount } => {
                if amount != tap_amount {
                    log::warn!("Mismatched tap amount between code and contract. Code: {tap_amount}. Contract: {amount}.")
                }
            }
        }
        Ok(())
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
    ) -> cosmos::Result<TxResponse> {
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
    ) -> cosmos::Result<TxResponse> {
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
    ) -> cosmos::Result<TxResponse> {
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
