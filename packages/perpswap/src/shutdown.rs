//! Types for market kill switch and winddown.
//!
//! These two mechanisms both allow authorized wallets to shut down parts of the
//! protocol, either at a market level or the entire protocol. Therefore they
//! share a set of types here.

use crate::prelude::*;
use std::collections::HashMap;

use cw_storage_plus::{Key, KeyDeserialize, PrimaryKey};
use once_cell::sync::Lazy;

/// Which wallet called the shutdown action?
#[derive(Debug, Clone, Copy)]
pub enum ShutdownWallet {
    /// The kill switch wallet
    KillSwitch,
    /// The wind down wallet
    WindDown,
}

/// Which part of the protocol should be impacted
#[cw_serde]
#[derive(enum_iterator::Sequence, Copy, Hash, Eq, PartialOrd, Ord)]
pub enum ShutdownImpact {
    /// Ability to open new positions and update existing positions.
    ///
    /// Includes: updating trigger orders, creating limit orders.
    NewTrades,
    /// Ability to close positions
    ClosePositions,
    /// Any owner actions on the market
    OwnerActions,
    /// Deposit liquidity, including reinvesting yield
    DepositLiquidity,
    /// Withdraw liquidity in any way
    ///
    /// Includes withdrawing, claiming yield
    WithdrawLiquidity,
    /// Any activities around xLP staking
    Staking,
    /// Any activities around unstaking xLP, including collecting
    Unstaking,
    /// Transfers of positions tokens
    TransferPositions,
    /// Transfers of liquidity tokens, both LP and xLP
    TransferLp,
    /// Setting the price
    SetPrice,
    /// Transfer DAO fees
    TransferDaoFees,
    /// Turning the crank
    Crank,
    /// Setting manual price
    SetManualPrice,
}

impl ShutdownImpact {
    /// Check if the wallet in question is allowed to perform the given action
    pub fn can_perform(self, shutdown_wallet: ShutdownWallet) -> bool {
        match (shutdown_wallet, self) {
            (ShutdownWallet::KillSwitch, _) => true,
            (ShutdownWallet::WindDown, ShutdownImpact::NewTrades) => true,
            (ShutdownWallet::WindDown, ShutdownImpact::ClosePositions) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::OwnerActions) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::DepositLiquidity) => true,
            (ShutdownWallet::WindDown, ShutdownImpact::WithdrawLiquidity) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::Staking) => true,
            (ShutdownWallet::WindDown, ShutdownImpact::Unstaking) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::TransferPositions) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::TransferLp) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::SetPrice) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::TransferDaoFees) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::Crank) => false,
            (ShutdownWallet::WindDown, ShutdownImpact::SetManualPrice) => false,
        }
    }

    /// Return an error if not allowed to perform
    pub fn ensure_can_perform(self, shutdown_wallet: ShutdownWallet) -> Result<()> {
        if self.can_perform(shutdown_wallet) {
            Ok(())
        } else {
            let msg = format!("{shutdown_wallet:?} cannot perform {self:?}");
            Err(anyhow!(PerpError::auth(ErrorDomain::Factory, msg)))
        }
    }

    /// Determines which shutdown impact, if any, gates the given market action
    pub fn for_market_execute_msg(
        msg: &crate::contracts::market::entry::ExecuteMsg,
    ) -> Option<Self> {
        use crate::contracts::market::entry::ExecuteMsg;
        match msg {
            ExecuteMsg::Owner(_) => Some(Self::OwnerActions),
            // Ignore here to avoid a double parse, and require this be checked explicitly in the market contract.
            ExecuteMsg::Receive { .. } => None,
            ExecuteMsg::OpenPosition { .. } => Some(Self::NewTrades),
            ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { .. } => Some(Self::NewTrades),
            ExecuteMsg::UpdatePositionAddCollateralImpactSize { .. } => Some(Self::NewTrades),
            ExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage { .. } => {
                Some(Self::NewTrades)
            }
            ExecuteMsg::UpdatePositionRemoveCollateralImpactSize { .. } => Some(Self::NewTrades),
            ExecuteMsg::UpdatePositionLeverage { .. } => Some(Self::NewTrades),
            ExecuteMsg::UpdatePositionMaxGains { .. } => Some(Self::NewTrades),
            ExecuteMsg::UpdatePositionTakeProfitPrice { .. } => Some(Self::NewTrades),
            ExecuteMsg::UpdatePositionStopLossPrice { .. } => Some(Self::NewTrades),
            #[allow(deprecated)]
            ExecuteMsg::SetTriggerOrder { .. } => Some(Self::NewTrades),
            ExecuteMsg::ClosePosition { .. } => Some(Self::ClosePositions),
            ExecuteMsg::DepositLiquidity { .. } => Some(Self::DepositLiquidity),
            ExecuteMsg::ReinvestYield { .. } => Some(Self::DepositLiquidity),
            ExecuteMsg::WithdrawLiquidity { .. } => Some(Self::WithdrawLiquidity),
            ExecuteMsg::ClaimYield {} => Some(Self::WithdrawLiquidity),
            ExecuteMsg::StakeLp { .. } => Some(Self::Staking),
            ExecuteMsg::UnstakeXlp { .. } => Some(Self::Unstaking),
            ExecuteMsg::StopUnstakingXlp {} => Some(Self::Unstaking),
            ExecuteMsg::CollectUnstakedLp {} => Some(Self::Unstaking),
            ExecuteMsg::Crank { .. } => Some(Self::Crank),
            ExecuteMsg::NftProxy { .. } => Some(Self::TransferPositions),
            ExecuteMsg::LiquidityTokenProxy { .. } => Some(Self::TransferLp),
            ExecuteMsg::TransferDaoFees { .. } => Some(Self::TransferDaoFees),
            ExecuteMsg::CloseAllPositions {} => None,
            ExecuteMsg::PlaceLimitOrder { .. } => Some(Self::NewTrades),
            ExecuteMsg::CancelLimitOrder { .. } => Some(Self::ClosePositions),
            ExecuteMsg::ProvideCrankFunds {} => Some(Self::Crank),
            ExecuteMsg::SetManualPrice { .. } => Some(Self::SetManualPrice),

            // Since this can only be executed by the contract itself, it's safe to never block it
            ExecuteMsg::PerformDeferredExec { .. } => None,
        }
    }
}

/// Are we turning off these features or turning them back on?
#[cw_serde]
#[derive(Copy)]
pub enum ShutdownEffect {
    /// Disable the given portion of the protocol
    Disable,
    /// Turn the given portion of the protocol back on
    Enable,
}

impl ShutdownImpact {
    /// Convert into a binary representation using the Debug impl.
    pub(crate) fn as_bytes(self) -> &'static [u8] {
        static LOOKUP: Lazy<HashMap<ShutdownImpact, Vec<u8>>> = Lazy::new(|| {
            enum_iterator::all::<ShutdownImpact>()
                .map(|x| (x, format!("{x:?}").into_bytes()))
                .collect()
        });
        LOOKUP
            .get(&self)
            .expect("Impossible! ShutdownImpact::as_bytes failed")
    }

    /// Parse this value from a binary representation of the Debug output.
    pub(crate) fn try_from_bytes(bytes: &[u8]) -> Result<Self> {
        static LOOKUP: Lazy<HashMap<Vec<u8>, ShutdownImpact>> = Lazy::new(|| {
            enum_iterator::all::<ShutdownImpact>()
                .map(|x| (format!("{x:?}").into_bytes(), x))
                .collect()
        });
        LOOKUP.get(bytes).copied().with_context(|| {
            format!(
                "Unable to parse as ShutdownImpact: {:?}",
                std::str::from_utf8(bytes)
            )
        })
    }
}

impl KeyDeserialize for ShutdownImpact {
    type Output = ShutdownImpact;

    const KEY_ELEMS: u16 = 1;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        ShutdownImpact::try_from_bytes(&value)
            .map_err(|x| cosmwasm_std::StdError::parse_err("ShutdownImpact", x))
    }
}

impl PrimaryKey<'_> for ShutdownImpact {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.as_bytes())]
    }
}
