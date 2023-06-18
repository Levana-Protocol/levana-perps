use cosmos::{
    proto::{cosmos::base::abci::v1beta1::TxResponse, cosmwasm::wasm::v1::MsgExecuteContract},
    Address, Contract, HasAddress, HasCosmos, TxBuilder, Wallet,
};

use cosmwasm_std::to_binary;
use msg::{
    contracts::{
        cw20::entry::BalanceResponse,
        market::{
            config::ConfigUpdate,
            entry::{
                ClosedPositionsResp, ExecuteOwnerMsg, LpInfoResp, PriceWouldTriggerResp,
                SlippageAssert, StatusResp,
            },
            position::{ClosedPosition, PositionId, PositionQueryResponse, PositionsResp},
        },
        position_token::entry::{NumTokensResponse, QueryMsg as PositionQueryMsg, TokensResponse},
    },
    prelude::*,
};

use crate::{PositionsInfo, UpdatePositionCollateralImpact};

pub struct MarketContract(Contract);

impl MarketContract {
    pub fn new(contract: Contract) -> Self {
        MarketContract(contract)
    }

    pub async fn status(&self) -> Result<StatusResp> {
        self.0.query(MarketQueryMsg::Status { price: None }).await
    }

    pub async fn status_at_height(&self, height: u64) -> Result<StatusResp> {
        self.0
            .query_at_height(MarketQueryMsg::Status { price: None }, height)
            .await
    }

    pub async fn current_price(&self) -> Result<PricePoint> {
        self.0
            .query(MarketQueryMsg::SpotPrice { timestamp: None })
            .await
    }

    pub(crate) async fn exec_with_funds(
        &self,
        wallet: &Wallet,
        status: &StatusResp,
        funds: NonZero<Collateral>,
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

    pub async fn deposit(
        &self,
        wallet: &Wallet,
        status: &StatusResp,
        funds: NonZero<Collateral>,
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

    pub async fn withdraw(
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

    pub async fn get_collateral_balance(
        &self,
        status: &StatusResp,
        addr: impl HasAddress,
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

    #[allow(clippy::too_many_arguments)]
    pub async fn open_position(
        &self,
        wallet: &Wallet,
        status: &StatusResp,
        deposit: NonZero<Collateral>,
        direction: DirectionToBase,
        leverage: LeverageToBase,
        max_gains: MaxGainsInQuote,
        slippage_assert: Option<SlippageAssert>,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<TxResponse> {
        let msg = MarketExecuteMsg::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
        };
        self.exec_with_funds(wallet, status, deposit, &msg)
            .await
            .with_context(|| {
                format!("Opening position with collateral {deposit} and parameters {msg:?}")
            })
    }

    pub async fn get_some_positions(
        &self,
        owner: Address,
        limit: Option<u32>,
    ) -> Result<Vec<PositionId>> {
        let TokensResponse { tokens } = self
            .0
            .query(MarketQueryMsg::NftProxy {
                nft_msg: msg::contracts::position_token::entry::QueryMsg::Tokens {
                    owner: owner.get_address_string().into(),
                    start_after: None,
                    limit,
                },
            })
            .await?;
        tokens
            .into_iter()
            .map(|x| x.parse().context("Invalid PositionId"))
            .collect()
    }

    pub async fn get_first_position(&self, owner: Address) -> Result<Option<PositionId>> {
        self.get_some_positions(owner, Some(1))
            .await
            .map(|x| x.into_iter().next())
    }

    pub async fn query_position(&self, pos_id: PositionId) -> Result<PositionQueryResponse> {
        let PositionsResp { mut positions, .. } = self
            .0
            .query(MarketQueryMsg::Positions {
                position_ids: vec![pos_id],
                skip_calc_pending_fees: Some(false),
                fees: None,
                price: None,
            })
            .await?;
        positions
            .pop()
            .with_context(|| format!("Could not query position #{pos_id}"))
    }

    pub async fn close_position(&self, wallet: &Wallet, pos: PositionId) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                MarketExecuteMsg::ClosePosition {
                    id: pos,
                    slippage_assert: None,
                },
            )
            .await
    }

    pub async fn lp_info(&self, addr: impl HasAddress) -> Result<LpInfoResp> {
        self.0
            .query(MarketQueryMsg::LpInfo {
                liquidity_provider: addr.get_address_string().into(),
            })
            .await
    }

    pub async fn close_positions(&self, wallet: &Wallet, positions: Vec<PositionId>) -> Result<()> {
        let mut builder = TxBuilder::default();
        for pos in positions {
            builder.add_message_mut(MsgExecuteContract {
                sender: wallet.get_address_string(),
                contract: self.0.get_address_string(),
                msg: serde_json::to_vec(&MarketExecuteMsg::ClosePosition {
                    id: pos,
                    slippage_assert: None,
                })?,
                funds: vec![],
            })
        }
        builder
            .sign_and_broadcast(self.0.get_cosmos(), wallet)
            .await?;
        Ok(())
    }

    pub async fn total_positions(&self) -> Result<u64> {
        let query = MarketQueryMsg::NftProxy {
            nft_msg: PositionQueryMsg::NumTokens {},
        };
        let response: NumTokensResponse = self.0.query(query).await?;
        Ok(response.count)
    }

    pub async fn all_open_positions(&self, owner: impl HasAddress) -> Result<PositionsInfo> {
        let mut start_after = None;
        let mut tokens = vec![];
        loop {
            let query = MarketQueryMsg::NftProxy {
                nft_msg: PositionQueryMsg::Tokens {
                    owner: owner.get_address_string().into(),
                    start_after: start_after.clone(),
                    limit: None,
                },
            };
            let mut response: TokensResponse = self.0.query(query).await?;
            match response.tokens.last() {
                Some(last_token) => start_after = Some(last_token.clone()),
                None => break,
            }
            tokens.append(&mut response.tokens);
        }
        let positions = tokens
            .iter()
            .map(|item| {
                item.parse()
                    .map_err(|_| anyhow!("Invalid position ID: {item}"))
            })
            .collect::<Result<Vec<PositionId>>>()?;

        let query = MarketQueryMsg::Positions {
            position_ids: positions.clone(),
            skip_calc_pending_fees: Some(false),
            fees: None,
            price: None,
        };
        let PositionsResp {
            positions: response,
            pending_close: _,
            closed: _,
        } = self.0.query(query).await?;
        assert_eq!(tokens.len(), response.len());
        let position_response = PositionsInfo {
            ids: positions,
            info: response,
        };
        Ok(position_response)
    }

    pub async fn position_detail(&self, position_id: PositionId) -> Result<PositionQueryResponse> {
        let query = MarketQueryMsg::Positions {
            position_ids: vec![position_id],
            skip_calc_pending_fees: Some(false),
            fees: None,
            price: None,
        };
        let PositionsResp {
            positions: mut response,
            pending_close: _,
            closed: _,
        } = self.0.query(query).await?;
        match response.pop() {
            Some(position) => Ok(position),
            None => Err(anyhow!("No position Id {position_id} found")),
        }
    }

    pub async fn set_price(&self, wallet: &Wallet, price: PriceBaseInQuote) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::SetPrice {
            price,
            price_usd: None,
            execs: None,
            rewards: None,
        };
        let response = self.0.execute(wallet, vec![], execute_msg).await?;
        Ok(response)
    }

    pub async fn get_closed_positions(
        &self,
        owner: impl HasAddress,
    ) -> Result<Vec<ClosedPosition>> {
        let mut result = vec![];
        let mut cursor = None;
        loop {
            let query_msg = MarketQueryMsg::ClosedPositionHistory {
                owner: owner.get_address_string().into(),
                cursor,
                limit: None,
                order: None,
            };
            let ClosedPositionsResp {
                mut positions,
                cursor: new_cursor,
            } = self.0.query(query_msg).await?;
            positions.sort_by(|a, b| a.id.cmp(&b.id));
            result.append(&mut positions);
            match new_cursor {
                Some(new_cursor) => cursor = Some(new_cursor),
                None => break,
            }
        }
        Ok(result)
    }

    pub async fn crank(&self, wallet: &Wallet) -> Result<()> {
        while self.status().await?.next_crank.is_some() {
            log::info!("Crank started");
            let execute_msg = MarketExecuteMsg::Crank {
                execs: None,
                rewards: None,
            };
            let tx = self.0.execute(wallet, vec![], execute_msg).await?;
            log::info!("{}", tx.txhash);
        }
        log::info!("Cranking finished");
        Ok(())
    }

    pub async fn update_max_gains(
        &self,
        wallet: &Wallet,
        id: PositionId,
        max_gains: MaxGainsInQuote,
    ) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::UpdatePositionMaxGains { id, max_gains };
        let response = self.0.execute(wallet, vec![], execute_msg).await?;
        Ok(response)
    }

    pub async fn update_leverage(
        &self,
        wallet: &Wallet,
        id: PositionId,
        leverage: LeverageToBase,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::UpdatePositionLeverage {
            id,
            leverage,
            slippage_assert,
        };
        let response = self.0.execute(wallet, vec![], execute_msg).await?;
        Ok(response)
    }

    pub async fn update_collateral(
        &self,
        wallet: &Wallet,
        id: PositionId,
        collateral: Collateral,
        impact: UpdatePositionCollateralImpact,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<TxResponse> {
        let status = self.status().await?;
        let position = self.query_position(id).await?;
        let active_collateral = position.active_collateral.into_number();
        if active_collateral == collateral.into_number() {
            bail!("No updated required since collateral is same");
        }
        if collateral.into_number() > active_collateral {
            log::info!("Increasing the collateral");

            let execute_msg = match impact {
                UpdatePositionCollateralImpact::Leverage => {
                    MarketExecuteMsg::UpdatePositionAddCollateralImpactLeverage { id }
                }
                UpdatePositionCollateralImpact::PositionSize => {
                    MarketExecuteMsg::UpdatePositionAddCollateralImpactSize {
                        id,
                        slippage_assert,
                    }
                }
            };

            let diff_collateral = collateral.into_number().checked_sub(active_collateral)?;
            let collateral = NonZero::<Collateral>::try_from_number(diff_collateral)
                .context("diff_collateral is not greater than zero")?;

            self.exec_with_funds(wallet, &status, collateral, &execute_msg)
                .await
        } else {
            log::info!("Decreasing the collateral");
            let diff_collateral = active_collateral.checked_sub(collateral.into_number())?;
            let amount: NonZero<Collateral> = {
                // for collateral removal, we need to be sure we're not hitting
                // the precision limit of the token
                let amount = NonZero::<Collateral>::try_from_number(diff_collateral)
                    .with_context(|| format!("Invalid diff_collateral: {diff_collateral}"))?;

                let amount = amount.into_decimal256();
                let amount = status
                    .collateral
                    .into_u128(amount)?
                    .context("zero after truncation")?;
                let amount = status.collateral.from_u128(amount)?;
                let amount = Collateral::from_decimal256(amount);
                NonZero::new(amount).context("zero after conversion")?
            };

            log::debug!("Diff collateral: {}", amount);
            let execute_msg = match impact {
                UpdatePositionCollateralImpact::Leverage => {
                    MarketExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage { id, amount }
                }
                UpdatePositionCollateralImpact::PositionSize => {
                    MarketExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                        id,
                        amount,
                        slippage_assert,
                    }
                }
            };
            self.0.execute(wallet, vec![], execute_msg).await
        }
    }

    pub async fn price_would_trigger(&self, price: PriceBaseInQuote) -> Result<bool> {
        let PriceWouldTriggerResp { would_trigger } = self
            .0
            .query(MarketQueryMsg::PriceWouldTrigger { price })
            .await?;
        Ok(would_trigger)
    }

    pub async fn config_update(&self, wallet: &Wallet, update: ConfigUpdate) -> Result<TxResponse> {
        self.0
            .execute(
                wallet,
                vec![],
                MarketExecuteMsg::Owner(ExecuteOwnerMsg::ConfigUpdate { update }),
            )
            .await
    }
}
