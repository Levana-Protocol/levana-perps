use std::str::FromStr;

use anyhow::{anyhow, Result};

#[derive(Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum ExchangeKind {
    Cex,
    Dex,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CmcExchangeInfo {
    pub(crate) data: CmcData,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CmcData {
    pub(crate) num_market_pairs: u32,
    pub(crate) market_pairs: Vec<CmcMarketPair>,
}

#[derive(serde::Deserialize, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CmcMarketPair {
    pub(crate) exchange_id: u32,
    pub(crate) exchange_name: String,
    pub(crate) market_pair: String,
    pub(crate) depth_usd_negative_two: f64,
    pub(crate) depth_usd_positive_two: f64,
    pub(crate) volume_percent: f64,
    /// 24 hour volume
    pub(crate) volume_usd: f64,
    pub(crate) market_reputation: f64,
    pub(crate) center_type: ExchangeKind,
}

#[derive(Debug, Copy, Clone, serde::Serialize, Hash, PartialEq, Eq)]
pub(crate) enum Coin {
    Atom,
    Levana,
    Eth,
}

impl FromStr for Coin {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ATOM" => Ok(Coin::Atom),
            "LEVANA" => Ok(Coin::Levana),
            "ETH" => Ok(Coin::Eth),
            other => Err(anyhow!("Unsupported coin {other}")),
        }
    }
}

impl Coin {
    pub(crate) fn cmc_id(&self) -> u32 {
        match self {
            // https://coinmarketcap.com/api/documentation/v1/#operation/getV1CryptocurrencyMap
            Coin::Atom => 3794,
            Coin::Levana => 28660,
            Coin::Eth => 1027,
        }
    }

    pub(crate) fn all() -> [Coin; 3] {
        [Coin::Atom, Coin::Levana, Coin::Eth]
    }
}
