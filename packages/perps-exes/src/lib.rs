pub mod config;
pub mod contracts;
pub mod discovery;
pub mod prelude;

use chrono::{DateTime, TimeZone, Utc};
use cosmos::{
    proto::cosmos::base::abci::v1beta1::TxResponse, Address, Contract, Cosmos, CosmosNetwork,
    HasAddress, HasAddressType, RawWallet, Wallet,
};
use msg::contracts::market::position::ClosedPosition;
use msg::prelude::*;
use msg::{
    contracts::{
        factory::entry::MarketInfoResponse,
        faucet::entry::{ExecuteMsg as FaucetExecuteMsg, FaucetAsset},
        market::{
            entry::SlippageAssert,
            position::{PositionId, PositionQueryResponse},
        },
    },
    token::Token,
};
use prelude::MarketContract;
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
    pub market: MarketContract,
    faucet_contract: Option<Contract>,
    pub cosmos: Cosmos,
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
        let builder = network.builder().await?;
        let cosmos = builder.build().await?;
        let factory_contract = Contract::new(cosmos.clone(), factory_contract_addr);

        let market_info: MarketInfoResponse = factory_contract
            .query(msg::contracts::factory::entry::QueryMsg::MarketInfo { market_id })
            .await?;
        let market_contract = cosmos.make_contract(market_info.market_addr.into_string().parse()?);

        let faucet_contract = faucet_contract_addr.map(|x| cosmos.make_contract(x));

        let wallet_address = *raw_wallet.for_chain(cosmos.get_address_type()).address();
        let address_type = cosmos.get_address_type();
        let wallet = raw_wallet.for_chain(address_type);
        Ok(PerpApp {
            wallet_address,
            raw_wallet,
            wallet,
            market: MarketContract::new(market_contract),
            faucet_contract,
            cosmos,
        })
    }

    pub async fn cw20_balance(&self) -> Result<Collateral> {
        let status = self.market.status().await?;
        self.market
            .get_collateral_balance(&status, &self.wallet)
            .await
    }

    pub async fn all_open_positions(&self) -> Result<PositionsInfo> {
        self.market.all_open_positions(&self.wallet).await
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
        self.market
            .open_position(
                &self.wallet,
                &self.market.status().await?,
                collateral,
                direction,
                leverage,
                max_gains,
                slippage_assert,
                stop_loss_override,
                take_profit_override,
            )
            .await
    }

    pub async fn deposit_liquidity(&self, collateral: NonZero<Collateral>) -> Result<TxResponse> {
        self.market
            .deposit(&self.wallet, &self.market.status().await?, collateral)
            .await
    }

    pub async fn set_price(&self, price: PriceBaseInQuote) -> Result<TxResponse> {
        self.market.set_price(&self.wallet, price).await
    }

    pub async fn get_closed_positions(&self) -> Result<Vec<ClosedPosition>> {
        self.market.get_closed_positions(&self.wallet).await
    }

    pub async fn close_position(&self, position_id: PositionId) -> Result<TxResponse> {
        self.market.close_position(&self.wallet, position_id).await
    }

    pub async fn crank(&self) -> Result<()> {
        self.market.crank(&self.wallet).await
    }

    pub async fn tap_faucet(&self) -> Result<TxResponse> {
        let cw20_address = match self.market.status().await?.collateral {
            Token::Cw20 {
                addr,
                decimal_places: _,
            } => FaucetAsset::Cw20(addr),
            Token::Native {
                denom,
                decimal_places: _,
            } => FaucetAsset::Native(denom),
        };
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
        id: PositionId,
        max_gains: MaxGainsInQuote,
    ) -> Result<TxResponse> {
        self.market
            .update_max_gains(&self.wallet, id, max_gains)
            .await
    }

    pub async fn update_leverage(
        &self,
        id: PositionId,
        leverage: LeverageToBase,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<TxResponse> {
        self.market
            .update_leverage(&self.wallet, id, leverage, slippage_assert)
            .await
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
        id: PositionId,
        collateral: Collateral,
        impact: UpdatePositionCollateralImpact,
        slippage_assert: Option<SlippageAssert>,
    ) -> Result<TxResponse> {
        self.market
            .update_collateral(&self.wallet, id, collateral, impact, slippage_assert)
            .await
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

/// Convert from CosmWasm to chrono representation of a timestamp.
pub fn timestamp_to_date_time(
    timestamp: impl Into<cosmwasm_std::Timestamp>,
) -> Result<DateTime<Utc>> {
    let timestamp = timestamp.into();
    Utc.timestamp_opt(
        timestamp.seconds().try_into()?,
        timestamp.subsec_nanos().try_into()?,
    )
    .single()
    .context("Could not convert last_crank_completed into DateTime<Utc>")
}
