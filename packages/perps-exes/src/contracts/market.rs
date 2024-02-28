use cosmos::{
    proto::{cosmos::base::abci::v1beta1::TxResponse, cosmwasm::wasm::v1::MsgExecuteContract},
    Address, Contract, HasAddress, HasAddressHrp, HasContract, HasCosmos, TxBuilder, Wallet,
};

use cosmwasm_std::to_binary;
use msg::{
    contracts::{
        cw20::entry::BalanceResponse,
        market::{
            config::{Config, ConfigUpdate},
            deferred_execution::{DeferredExecId, GetDeferredExecResp},
            entry::{
                ClosedPositionsResp, ExecuteOwnerMsg, LpAction, LpActionHistoryResp, LpInfoResp,
                OraclePriceResp, PositionAction, PositionActionHistoryResp, PriceWouldTriggerResp,
                SlippageAssert, StatusResp, TradeHistorySummary,
            },
            position::{ClosedPosition, PositionId, PositionQueryResponse, PositionsResp},
        },
        position_token::entry::{NumTokensResponse, QueryMsg as PositionQueryMsg, TokensResponse},
    },
    prelude::*,
};
use shared::namespace::{CLOSE_ALL_POSITIONS, LAST_POSITION_ID};

use crate::{PositionsInfo, UpdatePositionCollateralImpact};

#[derive(Clone)]
pub struct MarketContract(Contract);

impl Display for MarketContract {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl HasAddressHrp for MarketContract {
    fn get_address_hrp(&self) -> cosmos::AddressHrp {
        self.0.get_address_hrp()
    }
}
impl HasAddress for MarketContract {
    fn get_address(&self) -> Address {
        self.0.get_address()
    }
}
impl HasContract for MarketContract {
    fn get_contract(&self) -> &Contract {
        &self.0
    }
}
impl HasCosmos for MarketContract {
    fn get_cosmos(&self) -> &cosmos::Cosmos {
        self.0.get_cosmos()
    }
}

impl MarketContract {
    pub fn new(contract: Contract) -> Self {
        MarketContract(contract)
    }

    pub async fn status(&self) -> Result<StatusResp, cosmos::Error> {
        self.status_relaxed().await
    }

    /// Like status, but doesn't insist on the result being StatusResp.
    ///
    /// Useful for working around the overly aggressive cw_serde deny_unknown_fields.
    pub async fn status_relaxed<T: serde::de::DeserializeOwned>(&self) -> Result<T, cosmos::Error> {
        self.0.query(MarketQueryMsg::Status { price: None }).await
    }

    /// Get just the config out of the status
    pub async fn config(&self) -> Result<Config> {
        #[derive(serde::Deserialize)]
        struct SimpleStatus {
            config: Config,
        }
        let SimpleStatus { config } = self.status_relaxed().await?;
        Ok(config)
    }

    pub async fn status_at_height(&self, height: u64) -> Result<StatusResp, cosmos::Error> {
        // Maybe worth an improvement to cosmos-rs to make this nicer
        self.get_cosmos()
            .clone()
            .at_height(Some(height))
            .make_contract(self.0.get_address())
            .query(MarketQueryMsg::Status { price: None })
            .await
    }

    pub async fn current_price(&self) -> Result<PricePoint, cosmos::Error> {
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
        .map_err(|e| e.into())
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
    ) -> Result<TxResponse, cosmos::Error> {
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

    pub async fn close_position(
        &self,
        wallet: &Wallet,
        pos: PositionId,
    ) -> Result<TxResponse, cosmos::Error> {
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

    pub async fn lp_info(&self, addr: impl HasAddress) -> Result<LpInfoResp, cosmos::Error> {
        self.0
            .query(MarketQueryMsg::LpInfo {
                liquidity_provider: addr.get_address_string().into(),
            })
            .await
    }

    pub async fn close_positions(&self, wallet: &Wallet, positions: Vec<PositionId>) -> Result<()> {
        let mut builder = TxBuilder::default();
        for pos in positions {
            builder.add_message(MsgExecuteContract {
                sender: wallet.get_address_string(),
                contract: self.0.get_address_string(),
                msg: serde_json::to_vec(&MarketExecuteMsg::ClosePosition {
                    id: pos,
                    slippage_assert: None,
                })?,
                funds: vec![],
            });
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

    pub async fn all_closed_positions(
        &self,
        owner: impl HasAddress,
    ) -> Result<Vec<ClosedPosition>> {
        let mut cursor = None;
        let mut res = vec![];
        loop {
            let ClosedPositionsResp {
                mut positions,
                cursor: new_cursor,
            } = self
                .0
                .query(MarketQueryMsg::ClosedPositionHistory {
                    owner: owner.get_address_string().into(),
                    cursor: cursor.take(),
                    limit: None,
                    order: None,
                })
                .await?;
            res.append(&mut positions);
            match new_cursor {
                Some(new_cursor) => cursor = Some(new_cursor),
                None => break Ok(res),
            }
        }
    }

    pub async fn raw_query_positions(
        &self,
        position_ids: Vec<PositionId>,
    ) -> Result<PositionsResp, cosmos::Error> {
        self.0
            .query(MarketQueryMsg::Positions {
                position_ids,
                skip_calc_pending_fees: None,
                fees: None,
                price: None,
            })
            .await
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

    pub async fn set_price(
        &self,
        wallet: &Wallet,
        price: PriceBaseInQuote,
        price_usd: PriceCollateralInUsd,
    ) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::SetManualPrice { price, price_usd };

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

    pub async fn crank(&self, wallet: &Wallet, rewards: Option<RawAddr>) -> Result<()> {
        let mut status = self.status().await?;
        while status.next_crank.is_some() || status.deferred_execution_items != 0 {
            log::info!("Crank started");
            let execute_msg = MarketExecuteMsg::Crank {
                execs: None,
                rewards: rewards.clone(),
            };
            let tx = self.0.execute(wallet, vec![], execute_msg).await?;
            log::info!("{}", tx.txhash);
            status = self.status().await?;
        }
        log::info!("Cranking finished");
        Ok(())
    }

    pub async fn crank_single(
        &self,
        wallet: &Wallet,
        execs: Option<u32>,
        rewards: Option<RawAddr>,
    ) -> Result<TxResponse, cosmos::Error> {
        self.0
            .execute(wallet, vec![], MarketExecuteMsg::Crank { execs, rewards })
            .await
    }

    pub async fn update_max_gains(
        &self,
        wallet: &Wallet,
        id: PositionId,
        max_gains: MaxGainsInQuote,
    ) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::UpdatePositionMaxGains { id, max_gains };
        let status = self.status().await?;
        self.exec_with_crank_fee(wallet, &status, &execute_msg)
            .await
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
        let status = self.status().await?;
        self.exec_with_crank_fee(wallet, &status, &execute_msg)
            .await
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
            self.exec_with_crank_fee(wallet, &status, &execute_msg)
                .await
        }
    }

    async fn exec_with_crank_fee(
        &self,
        wallet: &Wallet,
        status: &StatusResp,
        msg: &MarketExecuteMsg,
    ) -> Result<TxResponse> {
        let price = self.current_price().await?;
        let crank_fee_usd = status.config.crank_fee_charged
            + status
                .config
                .crank_fee_surcharge
                .checked_mul_dec(Decimal256::from_ratio(
                    status.deferred_execution_items / 10,
                    1u32,
                ))?;
        let crank_fee = price.usd_to_collateral(crank_fee_usd);

        // Add a small multiplier to account for rounding errors. In real life
        // we'd use a larger multiplier to deal with price fluctuations too.
        let crank_fee = crank_fee.checked_mul_dec("1.01".parse().unwrap())?;

        match NonZero::new(crank_fee) {
            None => self
                .0
                .execute(wallet, vec![], msg)
                .await
                .map_err(|e| e.into()),
            Some(crank_fee) => self.exec_with_funds(wallet, status, crank_fee, msg).await,
        }
    }

    pub async fn price_would_trigger(&self, price: PriceBaseInQuote) -> Result<bool> {
        let PriceWouldTriggerResp { would_trigger } = self
            .0
            .query(MarketQueryMsg::PriceWouldTrigger { price })
            .await?;
        Ok(would_trigger)
    }

    pub async fn config_update(
        &self,
        wallet: &Wallet,
        update: ConfigUpdate,
    ) -> Result<TxResponse, cosmos::Error> {
        self.0
            .execute(
                wallet,
                vec![],
                MarketExecuteMsg::Owner(ExecuteOwnerMsg::ConfigUpdate {
                    update: Box::new(update),
                }),
            )
            .await
    }

    pub async fn close_all_positions(&self, wallet: &Wallet) -> Result<TxResponse, cosmos::Error> {
        self.0
            .execute(wallet, vec![], MarketExecuteMsg::CloseAllPositions {})
            .await
    }

    pub async fn trade_history_summary(
        &self,
        trader: Address,
    ) -> Result<TradeHistorySummary, cosmos::Error> {
        self.0
            .query(MarketQueryMsg::TradeHistorySummary {
                addr: trader.to_string().into(),
            })
            .await
    }

    pub async fn first_position_action(&self, id: PositionId) -> Result<Option<PositionAction>> {
        let PositionActionHistoryResp {
            actions,
            next_start_after: _,
        } = self
            .0
            .query(MarketQueryMsg::PositionActionHistory {
                id,
                start_after: None,
                limit: Some(1),
                order: Some(OrderInMessage::Ascending),
            })
            .await?;
        anyhow::ensure!(actions.len() <= 1);
        Ok(actions.into_iter().next())
    }

    pub async fn get_highest_position_id(&self) -> Result<PositionId> {
        // This should really be a proper query or part of StatusResp
        let bytes = self.0.query_raw(LAST_POSITION_ID).await?;
        serde_json::from_slice(&bytes).context("Invalid position ID")
    }

    pub async fn get_lp_actions(&self, lp: Address) -> Result<Vec<LpAction>> {
        let mut start_after = None;
        let mut res = vec![];

        loop {
            let LpActionHistoryResp {
                mut actions,
                next_start_after,
            } = self
                .0
                .query(MarketQueryMsg::LpActionHistory {
                    addr: lp.get_address_string().into(),
                    start_after: start_after.take(),
                    limit: None,
                    order: None,
                })
                .await?;
            res.append(&mut actions);
            if next_start_after.is_none() {
                break Ok(res);
            }
            start_after = next_start_after;
        }
    }

    pub async fn get_oracle_price(
        &self,
        validate_age: bool,
    ) -> Result<OraclePriceResp, cosmos::Error> {
        self.0
            .query(MarketQueryMsg::OraclePrice { validate_age })
            .await
    }

    pub async fn is_wound_down(&self) -> Result<bool, cosmos::Error> {
        self.0
            .query_raw(CLOSE_ALL_POSITIONS)
            .await
            .map(|v| !v.is_empty())
    }

    pub async fn get_deferred_exec(&self, id: DeferredExecId) -> Result<GetDeferredExecResp> {
        self.0
            .query(MarketQueryMsg::GetDeferredExec { id })
            .await
            .map_err(|e| e.into())
    }
}
