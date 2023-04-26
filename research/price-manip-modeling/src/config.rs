use anyhow::Result;

use crate::types::{Asset, Price, Usd};

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct Config {
    /// Amount of USD per block we believe will be leveraged to arbitrage the system back to expected spot
    pub(crate) arbitrage_rate: Usd,

    /// Epsilon percentage under which we don't bother doing any arbitrage
    ///
    /// This is a number like 0.0005, which would represent 0.05% epsiolon
    pub(crate) arbitrage_epsilon: f64,

    /// Original spot price that arbitragers want to push the price back towards
    pub(crate) expected_spot: Price,

    /// Starting AMM balance for the asset
    pub(crate) amm_volume: Asset,

    /// How many blocks the TWAP averaging covers
    pub(crate) twap_blocks: usize,

    /// Do we leverage a "the house always wins" approach for calculating entry and exit prices?
    pub(crate) house_wins: bool,

    /// Maximum leverage we allow. We assume the attacker always uses max
    pub(crate) max_leverage: f64,

    /// Trading fees charged on trader collateral
    pub(crate) trading_fee_rate: f64,

    /// Trading fees charged on counter side collateral
    pub(crate) cs_trading_fee_rate: f64,

    /// Slippage cap parameter
    pub(crate) slippage_cap: f64,

    /// Slippage K parameter
    pub(crate) slippage_k: Asset,

    /// How slippage is handled in the protocol
    pub(crate) slippage: SlippageRules,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SlippageRules {
    NoSlippage,
    /// Trader receives full slippage amount
    TraderFullSlippage,
    /// Trader only receives half the slippage amount
    TraderHalfSlippage,
    /// Trader never receives slippage, only pays it
    UnidirectionalSlippage,
}

impl SlippageRules {
    pub(crate) fn all() -> [Self; 4] {
        use SlippageRules::*;
        [
            NoSlippage,
            TraderFullSlippage,
            TraderHalfSlippage,
            UnidirectionalSlippage,
        ]
    }
}

impl Config {
    pub(crate) fn check_max_leverage(&self, value: f64) -> Result<()> {
        // Use an espilon to deal with rounding errors
        if value - self.max_leverage >= 0.0001 {
            Err(anyhow::anyhow!(
                "Max leverage check failed for value: {value}"
            ))
        } else {
            Ok(())
        }
    }
}

// Initial values were taken on October 19, 2022 from https://app.osmosis.zone/pool/678
pub(crate) const INITIAL_USD: Usd = Usd(14200249.929888);
pub(crate) const INITIAL_ASSET: Asset = Asset(11863474.335278);
pub(crate) const INITIAL_PRICE: Price = Price(INITIAL_USD.0 / INITIAL_ASSET.0);

impl Default for Config {
    fn default() -> Self {
        // Should be the amount of money it takes to double the spot price.
        let slippage_k = Asset(INITIAL_ASSET.0 - INITIAL_ASSET.0 / (2.0f64).sqrt());
        assert!(slippage_k.0 >= 0.0);
        Self {
            arbitrage_rate: Usd(100.0),
            arbitrage_epsilon: 0.0005,
            expected_spot: INITIAL_PRICE,
            amm_volume: INITIAL_ASSET,
            twap_blocks: 1,
            house_wins: false,
            max_leverage: 30.0,
            trading_fee_rate: 0.0005,
            cs_trading_fee_rate: 0.001,
            slippage_cap: 0.005,
            slippage_k,
            slippage: SlippageRules::TraderFullSlippage,
        }
    }
}
