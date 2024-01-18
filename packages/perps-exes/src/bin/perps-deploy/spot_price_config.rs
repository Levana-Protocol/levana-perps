use anyhow::{Context, Result};
use cosmos::HasAddress;
use msg::contracts::market::spot_price::{
    PythConfigInit, PythPriceServiceNetwork, SpotPriceConfigInit, StrideConfigInit,
};
use perps_exes::config::PriceConfig;
use shared::storage::MarketId;

use crate::app::OracleInfo;

pub(crate) fn get_spot_price_config(
    oracle: &OracleInfo,
    price_config: &PriceConfig,
    market_id: &MarketId,
) -> Result<SpotPriceConfigInit> {
    let market = oracle
        .markets
        .get(&market_id)
        .with_context(|| format!("No oracle market found for {market_id}"))?;
    let stride = match market.stride_contract_override {
        Some(stride) => Some(stride),
        None => oracle.stride_fallback.clone().map(|stride| stride.contract),
    };
    Ok(SpotPriceConfigInit::Oracle {
        pyth: oracle.pyth.as_ref().map(|pyth| PythConfigInit {
            contract_address: pyth.contract.get_address_string().into(),
            network: pyth.r#type,
        }),
        stride: stride.map(|addr| StrideConfigInit {
            contract_address: addr.get_address_string().into(),
        }),
        feeds: market
            .feeds
            .iter()
            .map(|feed| feed.clone().into())
            .collect(),
        feeds_usd: market
            .feeds_usd
            .iter()
            .map(|feed| feed.clone().into())
            .collect(),
        volatile_diff_seconds: None,
    })
}
