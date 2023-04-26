pub mod config;
pub mod types;
pub mod wallet_manager;

use cosmos::{
    proto::cosmos::base::abci::v1beta1::TxResponse, Address, Contract, Cosmos, CosmosNetwork,
    HasAddress, HasAddressType, RawWallet, Wallet,
};
use msg::contracts::market::crank::CrankWorkInfo;
use msg::contracts::market::entry::StatusResp;
use msg::contracts::market::{config::Config, position::ClosedPosition};
use msg::prelude::*;
use msg::{
    contracts::{
        cw20::entry::{BalanceResponse, ExecuteMsg as Cw20ExecuteMsg, QueryMsg as Cw20QueryMsg},
        factory::entry::MarketInfoResponse,
        faucet::entry::{ExecuteMsg as FaucetExecuteMsg, FaucetAsset},
        market::{
            entry::{
                ClosedPositionsResp, ExecuteMsg as MarketExecuteMsg, QueryMsg as MarketQueryMsg,
                SlippageAssert,
            },
            liquidity::LiquidityStats,
            position::{PositionId, PositionQueryResponse, PositionsResp},
        },
        position_token::entry::{NumTokensResponse, QueryMsg as PositionQueryMsg, TokensResponse},
    },
    token::Token,
};
use std::time::Duration;

/// Get the Git SHA from GitHub Actions env vars
pub fn build_version() -> &'static str {
    const BUILD_VERSION: Option<&str> = option_env!("GITHUB_SHA");
    BUILD_VERSION.unwrap_or("Local build")
}

pub struct PerpApp {
    pub wallet_address: Address,
    pub raw_wallet: RawWallet,
    pub wallet: Wallet,
    cw20_contract: Contract,
    pub market_contract: Contract,
    faucet_contract: Option<Contract>,
    cosmos: Cosmos,
    token: Token,
}

pub struct PositionsInfo {
    pub ids: Vec<PositionId>,
    pub info: Vec<PositionQueryResponse>,
}

impl PerpApp {
    pub async fn new(
        raw_wallet: RawWallet,
        factory_contract_addr: Address,
        faucet_contract_addr: Option<Address>,
        market_id: MarketId,
        network: CosmosNetwork,
    ) -> Result<PerpApp> {
        let builder = network.builder();
        let cosmos = builder.build().await?;
        let factory_contract = Contract::new(cosmos.clone(), factory_contract_addr);

        let market_info: MarketInfoResponse = factory_contract
            .query(msg::contracts::factory::entry::QueryMsg::MarketInfo { market_id })
            .await?;
        let market_contract = cosmos.make_contract(market_info.market_addr.into_string().parse()?);

        let faucet_contract = faucet_contract_addr.map(|x| cosmos.make_contract(x));

        let status: StatusResp = market_contract
            .query(msg::contracts::market::entry::QueryMsg::Status {})
            .await?;
        let cw20_contract = match &status.collateral {
            Token::Cw20 {
                addr,
                decimal_places: _,
            } => cosmos.make_contract(addr.as_str().parse()?),
            Token::Native { .. } => anyhow::bail!("No support for native coins for the moment"),
        };

        let wallet_address = *raw_wallet.for_chain(cosmos.get_address_type()).address();
        let address_type = cosmos.get_address_type();
        let wallet = raw_wallet.for_chain(address_type);
        Ok(PerpApp {
            wallet_address,
            raw_wallet,
            wallet,
            market_contract,
            faucet_contract,
            cw20_contract,
            cosmos,
            token: status.collateral,
        })
    }

    pub async fn cw20_balance(&self) -> Result<BalanceResponse> {
        let message = Cw20QueryMsg::Balance {
            address: self.wallet_address.get_address_string().into(),
        };
        let response: BalanceResponse = self.cw20_contract.query(message).await?;
        Ok(response)
    }

    pub async fn total_positions(&self) -> Result<u64> {
        let query = MarketQueryMsg::NftProxy {
            nft_msg: PositionQueryMsg::NumTokens {},
        };
        let response: NumTokensResponse = self.market_contract.query(query).await?;
        Ok(response.count)
    }

    pub async fn all_open_positions(&self) -> Result<PositionsInfo> {
        let mut start_after = None;
        let mut tokens = vec![];
        loop {
            let query = MarketQueryMsg::NftProxy {
                nft_msg: PositionQueryMsg::Tokens {
                    owner: self.wallet_address.to_string().into(),
                    start_after: start_after.clone(),
                    limit: None,
                },
            };
            let mut response: TokensResponse = self.market_contract.query(query).await?;
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
                    .map(PositionId)
                    .map_err(|_| anyhow!("Invalid position ID: {item}"))
            })
            .collect::<Result<Vec<PositionId>>>()?;

        let query = MarketQueryMsg::Positions {
            position_ids: positions.clone(),
            skip_calc_pending_fees: false,
        };
        let PositionsResp {
            positions: response,
            pending_close: _,
        } = self.market_contract.query(query).await?;
        assert_eq!(tokens.len(), response.len());
        let position_response = PositionsInfo {
            ids: positions,
            info: response,
        };
        Ok(position_response)
    }

    pub async fn position_detail(&self, position_id: u64) -> Result<PositionQueryResponse> {
        let query = MarketQueryMsg::Positions {
            position_ids: vec![PositionId(position_id)],
            skip_calc_pending_fees: false,
        };
        let PositionsResp {
            positions: mut response,
            pending_close: _,
        } = self.market_contract.query(query).await?;
        match response.pop() {
            Some(position) => Ok(position),
            None => Err(anyhow!("No position Id {position_id} found")),
        }
    }

    /// Convert collateral into a u128
    pub fn collateral_to_u128(&self, amount: NonZero<Collateral>) -> Result<u128> {
        self.token
            .into_u128(amount.into_decimal256())?
            .with_context(|| format!("collateral_to_u128: invalid amount {amount}"))
    }

    /// Execute a message against the market with the given amount of funds send.
    ///
    /// In the future, check if this is native or CW20 and handle appropriately. For now, just handles CW20.
    pub(crate) async fn market_execute_with_funds(
        &self,
        market_execute_msg: &MarketExecuteMsg,
        amount: NonZero<Collateral>,
    ) -> Result<TxResponse> {
        let cw20_execute_msg = Cw20ExecuteMsg::Send {
            contract: self.market_contract.get_address_string().into(),
            amount: self.collateral_to_u128(amount)?.into(),
            msg: serde_json::to_vec(market_execute_msg)?.into(),
        };
        self.cw20_contract
            .execute(&self.wallet, vec![], cw20_execute_msg)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn open_position(
        &self,
        collateral: NonZero<Collateral>,
        direction: DirectionToBase,
        leverage: LeverageToBase,
        max_gains: MaxGainsInQuote,
        slippage_assert: Option<SlippageAssert>,
        stop_loss_override: Option<PriceBaseInQuote>,
        take_profit_override: Option<PriceBaseInQuote>,
    ) -> Result<TxResponse> {
        let open_message = MarketExecuteMsg::OpenPosition {
            slippage_assert,
            leverage,
            direction,
            max_gains,
            stop_loss_override,
            take_profit_override,
        };
        self.market_execute_with_funds(&open_message, collateral)
            .await
            .with_context(|| format!("Opening position with parameters: {open_message:?}"))
    }

    pub async fn deposit_liquidity(&self, collateral: NonZero<Collateral>) -> Result<TxResponse> {
        self.market_execute_with_funds(
            &MarketExecuteMsg::DepositLiquidity {
                stake_to_xlp: false,
            },
            collateral,
        )
        .await
    }

    pub async fn fetch_price(&self) -> Result<PricePoint> {
        let query = MarketQueryMsg::SpotPrice { timestamp: None };
        let response = self.market_contract.query(query).await?;
        Ok(response)
    }

    pub async fn set_price(&self, price: PriceBaseInQuote) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::SetPrice {
            price,
            price_usd: None,
            execs: None,
            rewards: None,
        };
        let response = self
            .market_contract
            .execute(&self.wallet, vec![], execute_msg)
            .await?;
        Ok(response)
    }

    pub async fn get_close_positions(&self) -> Result<Vec<ClosedPosition>> {
        let owner = self.wallet_address.to_string();
        let mut result = vec![];
        let mut cursor = None;
        loop {
            let query_msg = MarketQueryMsg::ClosedPositionHistory {
                owner: owner.clone().into(),
                cursor,
                limit: None,
                order: None,
            };
            let ClosedPositionsResp {
                mut positions,
                cursor: new_cursor,
            } = self.market_contract.query(query_msg).await?;
            positions.sort_by(|a, b| a.id.cmp(&b.id));
            result.append(&mut positions);
            match new_cursor {
                Some(new_cursor) => cursor = Some(new_cursor),
                None => break,
            }
        }
        Ok(result)
    }

    pub async fn close_position(&self, position_id: u64) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::ClosePosition {
            id: PositionId(position_id),
            slippage_assert: None,
        };
        let response = self
            .market_contract
            .execute(&self.wallet, vec![], execute_msg)
            .await?;
        Ok(response)
    }

    pub async fn status(&self) -> Result<StatusResp> {
        let query_msg = MarketQueryMsg::Status {};
        self.market_contract.query(&query_msg).await
    }

    async fn crank_stats(&self) -> Result<Option<CrankWorkInfo>> {
        self.status().await.map(|x| x.next_crank)
    }

    pub async fn crank(&self) -> Result<()> {
        while self.crank_stats().await?.is_some() {
            log::info!("Crank started");
            let execute_msg = MarketExecuteMsg::Crank {
                execs: None,
                rewards: None,
            };
            let tx = self
                .market_contract
                .execute(&self.wallet, vec![], execute_msg)
                .await?;
            log::info!("{}", tx.txhash);
        }
        log::info!("Cranking finished");
        Ok(())
    }

    pub async fn tap_faucet(&self) -> Result<TxResponse> {
        let cw20_address = FaucetAsset::Cw20(self.cw20_contract.get_address_string().into());
        let execute_msg = FaucetExecuteMsg::Tap {
            assets: vec![cw20_address],
            recipient: self.wallet_address.get_address_string().into(),
            amount: None,
        };
        self.faucet_contract
            .as_ref()
            .context("No faucet available")?
            .execute(&self.wallet, vec![], execute_msg)
            .await
    }

    pub async fn update_max_gains(
        &self,
        id: u64,
        max_gains: MaxGainsInQuote,
    ) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::UpdatePositionMaxGains {
            id: PositionId(id),
            max_gains,
        };
        let response = self
            .market_contract
            .execute(&self.wallet, vec![], execute_msg)
            .await?;
        Ok(response)
    }

    pub async fn update_leverage(
        &self,
        id: u64,
        leverage: LeverageToBase,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<TxResponse> {
        let execute_msg = MarketExecuteMsg::UpdatePositionLeverage {
            id: PositionId(id),
            leverage,
            slippage_assert,
        };
        let response = self
            .market_contract
            .execute(&self.wallet, vec![], execute_msg)
            .await?;
        Ok(response)
    }

    pub async fn wait_till_next_block(&self) -> Result<()> {
        let block = self.cosmos.get_latest_block_info().await?;
        let current_height = block.height;
        let total_duration = Duration::from_secs(10);
        let step_duration = Duration::from_millis(50);
        let total_iteration = total_duration.as_millis() / step_duration.as_millis();
        for _ in 0..total_iteration {
            let new_block = self.cosmos.get_latest_block_info().await?;
            if new_block.height != current_height {
                return Ok(());
            }
            tokio::time::sleep(step_duration).await;
        }
        Err(anyhow!("No new blocks after 10 seconds"))
    }

    pub async fn update_collateral(
        &self,
        position_id: u64,
        collateral: Collateral,
        impact: UpdatePositionCollateralImpact,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<TxResponse> {
        let query = MarketQueryMsg::Positions {
            position_ids: vec![PositionId(position_id)],
            skip_calc_pending_fees: false,
        };
        let PositionsResp {
            positions: mut response,
            pending_close: _,
        } = self.market_contract.query(query).await?;
        let position = match response.pop() {
            Some(position) => Ok(position),
            None => Err(anyhow!("No position Id {position_id} found")),
        }?;
        let active_collateral = position.active_collateral.into_number();
        if active_collateral == collateral.into_number() {
            bail!("No updated required since collateral is same");
        }
        if collateral.into_number() > active_collateral {
            log::info!("Increasing the collateral");

            let execute_msg = match impact {
                UpdatePositionCollateralImpact::Leverage => {
                    MarketExecuteMsg::UpdatePositionAddCollateralImpactLeverage {
                        id: PositionId(position_id),
                    }
                }
                UpdatePositionCollateralImpact::PositionSize => {
                    MarketExecuteMsg::UpdatePositionAddCollateralImpactSize {
                        id: PositionId(position_id),
                        slippage_assert,
                    }
                }
            };

            let diff_collateral = collateral.into_number().checked_sub(active_collateral)?;
            let collateral = NonZero::<Collateral>::try_from_number(diff_collateral)
                .context("diff_collateral is not greater than zero")?;

            self.market_execute_with_funds(&execute_msg, collateral)
                .await
        } else {
            log::info!("Decreasing the collateral");
            let diff_collateral = active_collateral.checked_sub(collateral.into_number())?;
            let amount = NonZero::<Collateral>::try_from_number(diff_collateral)
                .with_context(|| format!("Invalid diff_collateral: {diff_collateral}"))?;
            log::debug!("Diff collateral: {}", amount);
            let execute_msg = match impact {
                UpdatePositionCollateralImpact::Leverage => {
                    MarketExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage {
                        id: PositionId(position_id),
                        amount,
                    }
                }
                UpdatePositionCollateralImpact::PositionSize => {
                    MarketExecuteMsg::UpdatePositionRemoveCollateralImpactSize {
                        id: PositionId(position_id),
                        amount,
                        slippage_assert,
                    }
                }
            };
            self.market_contract
                .execute(&self.wallet, vec![], execute_msg)
                .await
        }
    }

    pub async fn liquidity_stats(&self) -> Result<LiquidityStats> {
        self.status().await.map(|x| x.liquidity)
    }

    pub async fn get_config(&self) -> Result<Config> {
        self.status().await.map(|x| x.config)
    }
}

#[derive(Copy, Clone)]
pub enum UpdatePositionCollateralImpact {
    Leverage,
    PositionSize,
}

impl FromStr for UpdatePositionCollateralImpact {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &s.to_lowercase()[..] {
            "leverage" => Ok(UpdatePositionCollateralImpact::Leverage),
            "positionsize" => Ok(UpdatePositionCollateralImpact::PositionSize),
            other => Err(anyhow!(
                "Invalid value, should be either leverage or positionsize. Instead got {other}"
            )),
        }
    }
}
