use cosmos::{
    proto::cosmos::base::abci::v1beta1::TxResponse, Address, Contract, HasAddress, HasCosmos,
    Wallet,
};

use cosmwasm_std::to_binary;
use msg::{
    contracts::{
        cw20::entry::BalanceResponse,
        market::{
            entry::{LpInfoResp, StatusResp},
            position::PositionId,
        },
        position_token::entry::TokensResponse,
    },
    prelude::*,
};

pub(crate) struct MarketContract(Contract);

impl MarketContract {
    pub(crate) fn new(contract: Contract) -> Self {
        MarketContract(contract)
    }

    pub(crate) async fn status(&self) -> Result<StatusResp> {
        self.0.query(MarketQueryMsg::Status {}).await
    }

    async fn exec_with_funds(
        &self,
        wallet: &Wallet,
        status: &StatusResp,
        funds: Collateral,
        msg: &MarketExecuteMsg,
    ) -> Result<TxResponse> {
        let funds = status
            .collateral
            .into_u128(funds.into_decimal256())?
            .context("exec_with_funds: no funds")?;
        let cw20 = match &status.collateral {
            msg::token::Token::Cw20 {
                addr,
                decimal_places: _,
            } => addr.as_str().parse()?,
            msg::token::Token::Native { .. } => anyhow::bail!("No support for native"),
        };
        let cw20 = self.0.get_cosmos().make_contract(cw20);
        cw20.execute(
            wallet,
            vec![],
            msg::contracts::cw20::entry::ExecuteMsg::Send {
                contract: self.0.get_address_string().into(),
                amount: funds.into(),
                msg: to_binary(msg)?,
            },
        )
        .await
    }

    pub(crate) async fn deposit(
        &self,
        wallet: &Wallet,
        status: &StatusResp,
        funds: Collateral,
    ) -> Result<TxResponse> {
        self.exec_with_funds(
            wallet,
            status,
            funds,
            &MarketExecuteMsg::DepositLiquidity {
                stake_to_xlp: false,
            },
        )
        .await
    }

    pub(crate) async fn withdraw(
        &self,
        wallet: &Wallet,
        lp_tokens: NonZero<LpToken>,
    ) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                &MarketExecuteMsg::WithdrawLiquidity {
                    lp_amount: Some(lp_tokens),
                },
            )
            .await
    }

    pub(crate) async fn get_collateral_balance(
        &self,
        status: &StatusResp,
        addr: Address,
    ) -> Result<Collateral> {
        let cw20 = match &status.collateral {
            msg::token::Token::Cw20 {
                addr,
                decimal_places: _,
            } => addr.as_str().parse()?,
            msg::token::Token::Native { .. } => anyhow::bail!("No support for native"),
        };
        let cw20 = self.0.get_cosmos().make_contract(cw20);
        let BalanceResponse { balance } = cw20
            .query(msg::contracts::cw20::entry::QueryMsg::Balance {
                address: addr.get_address_string().into(),
            })
            .await?;
        status
            .collateral
            .from_u128(balance.u128())
            .map(Collateral::from_decimal256)
    }

    pub(crate) async fn open_position(
        &self,
        wallet: &Wallet,
        status: &StatusResp,
        deposit: Collateral,
        direction: DirectionToBase,
        leverage: LeverageToBase,
        max_gains: MaxGainsInQuote,
    ) -> Result<()> {
        self.exec_with_funds(
            wallet,
            status,
            deposit,
            &MarketExecuteMsg::OpenPosition {
                slippage_assert: None,
                leverage,
                direction,
                max_gains,
                stop_loss_override: None,
                take_profit_override: None,
            },
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn get_first_position(&self, owner: Address) -> Result<Option<PositionId>> {
        let TokensResponse { mut tokens } = self
            .0
            .query(MarketQueryMsg::NftProxy {
                nft_msg: msg::contracts::position_token::entry::QueryMsg::Tokens {
                    owner: owner.get_address_string().into(),
                    start_after: None,
                    limit: Some(1),
                },
            })
            .await?;
        tokens
            .pop()
            .map(|x| x.parse().context("Invalid PositionId"))
            .transpose()
    }

    pub(crate) async fn close_position(&self, wallet: &Wallet, pos: PositionId) -> Result<()> {
        self.0
            .execute(
                wallet,
                vec![],
                MarketExecuteMsg::ClosePosition {
                    id: pos,
                    slippage_assert: None,
                },
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn lp_info(&self, addr: impl HasAddress) -> Result<LpInfoResp> {
        self.0
            .query(MarketQueryMsg::LpInfo {
                liquidity_provider: addr.get_address_string().into(),
            })
            .await
    }
}
