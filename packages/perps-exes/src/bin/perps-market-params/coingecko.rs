use std::str::FromStr;

use anyhow::{anyhow, Result};
use serde::{
    de::{Unexpected, Visitor},
    Deserialize,
};
use shared::storage::MarketId;

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

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CmcMarketPair {
    pub(crate) exchange_id: ExchangeId,
    pub(crate) exchange_name: String,
    pub(crate) market_id: MarketId,
    pub(crate) depth_usd_negative_two: f64,
    pub(crate) depth_usd_positive_two: f64,
    pub(crate) volume_24h_usd: f64,
    pub(crate) outlier_detected: f64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Clone, PartialOrd, Eq, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) struct ExchangeId(pub(crate) u32);

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Clone, PartialOrd, Eq, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CMCExchange {
    pub(crate) id: ExchangeId,
    pub(crate) name: String,
    pub(crate) slug: String,
}

impl ExchangeId {
    pub(crate) fn exchange_type(&self) -> anyhow::Result<ExchangeKind> {
        match self.0 {
            339 | 270 | 407 | 302 | 102 | 521 | 294 | 24 | 36 | 89 | 125 | 151 | 154 | 16 | 37
            | 42 | 50 | 68 | 82 | 194 | 200 | 325 | 9665 | 9584 | 9450 | 9449 | 9200 | 8961
            | 8884 | 9606 | 9365 | 9218 | 9202 | 9181 | 8885 | 8563 | 8125 | 7850 | 7618 | 7557
            | 7451 | 7373 | 7302 | 7085 | 6472 | 6406 | 350 | 370 | 949 | 622 | 425 | 354 | 376
            | 433 | 1289 | 943 | 1133 | 790 | 857 | 1145 | 1009 | 1199 | 5590 | 1247 | 1188
            | 157 | 174 | 215 | 225 | 243 | 253 | 311 | 330 | 333 | 351 | 378 | 380 | 406 | 415
            | 436 | 440 | 443 | 453 | 468 | 477 | 487 | 488 | 500 | 501 | 513 | 517 | 520 | 525
            | 535 | 544 | 549 | 585 | 594 | 630 | 658 | 710 | 795 | 802 | 834 | 867 | 937 | 955
            | 988 | 1006 | 1064 | 1124 | 1149 | 1182 | 1215 | 1375 | 1531 | 1561 | 1645 | 3735
            | 5319 | 6137 | 7748 | 8080 | 8767 | 9217 | 9384 => Ok(ExchangeKind::Cex),
            1707 | 1454 | 1187 | 1530 | 1567 | 1344 | 1714 | 9244 | 1165 | 1293 | 1327 | 1395
            | 1447 | 1547 | 1551 | 1614 | 1657 | 1665 | 6255 | 6444 | 6706 | 6757 | 8915 | 9245 => {
                Ok(ExchangeKind::Dex)
            }
            other => Err(anyhow!("Exchange type not known for id {}", other)),
        }
    }
}

impl<'de> Deserialize<'de> for CmcMarketPair {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Result {
            exchange: CmcExchange,
            market_pair: String,
            outlier_detected: f64,
            quote: CmcQuote,
        }

        #[derive(Deserialize)]
        struct CmcQuote {
            #[serde(rename(deserialize = "USD"))]
            usd: QuoteUsd,
        }

        #[derive(Deserialize)]
        struct QuoteUsd {
            // Occasionally volume can be excluded for a certain exchange
            volume_24h: f64,
            depth_negative_two: Option<f64>,
            depth_positive_two: Option<f64>,
        }

        #[derive(Deserialize)]
        struct CmcExchange {
            id: u32,
            name: String,
        }

        let result = Result::deserialize(deserializer)?;

        let market_pair = result.market_pair.replace("/", "_");
        let market_pair = MarketId::from_str(&market_pair).map_err(|_| {
            serde::de::Error::invalid_value(Unexpected::Str(&market_pair), &"Valid market id")
        })?;

        Ok(CmcMarketPair {
            exchange_id: ExchangeId(result.exchange.id),
            exchange_name: result.exchange.name,
            market_id: market_pair,
            depth_usd_negative_two: result.quote.usd.depth_negative_two.unwrap_or_default(),
            depth_usd_positive_two: result.quote.usd.depth_positive_two.unwrap_or_default(),
            volume_24h_usd: result.quote.usd.volume_24h,
            outlier_detected: result.outlier_detected,
        })
    }
}

// todo: put convert = usd

#[derive(Debug, Copy, Clone, serde::Serialize, Hash, PartialEq, Eq)]
pub(crate) enum Coin {
    Atom,
    Levana,
    Eth,
    Dogecoin,
    Wbtc,
    Avax,
    Dot,
    Btc,
    StAtom,
    StDYDX,
    Bnb,
    Luna,
    Dym,
    Osmo,
    Link,
    Sol,
    Sei,
    Pyth,
    Silver,
    Dydx,
    Inj,
    StTia,
}

impl FromStr for Coin {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ATOM" => Ok(Coin::Atom),
            "LEVANA" => Ok(Coin::Levana),
            "ETH" => Ok(Coin::Eth),
            "DOGE" => Ok(Coin::Dogecoin),
            "WBTC" => Ok(Coin::Wbtc),
            "AVAX" => Ok(Coin::Avax),
            "DOT" => Ok(Coin::Dot),
            "BTC" => Ok(Coin::Btc),
            "stATOM" => Ok(Coin::StAtom),
            "stDYDX" => Ok(Coin::StDYDX),
            "BNB" => Ok(Coin::Bnb),
            "LUNA" => Ok(Coin::Luna),
            "DYM" => Ok(Coin::Dym),
            "OSMO" => Ok(Coin::Osmo),
            "LINK" => Ok(Coin::Link),
            "SOL" => Ok(Coin::Sol),
            "SEI" => Ok(Coin::Sei),
            "PYTH" => Ok(Coin::Pyth),
            "SILVER" => Ok(Coin::Silver),
            "DYDX" => Ok(Coin::Dydx),
            "INJ" => Ok(Coin::Inj),
            "stTIA" => Ok(Coin::StTia),
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
            Coin::Dogecoin => 74,
            Coin::Wbtc => 3717,
            Coin::Avax => 5805,
            Coin::Dot => 6636,
            Coin::Btc => 1,
            Coin::StAtom => 21686,
            Coin::StDYDX => 29428,
            Coin::Bnb => 1839,
            Coin::Luna => 20314,
            Coin::Dym => 28932,
            Coin::Osmo => 12220,
            Coin::Link => 1975,
            Coin::Sol => 5426,
            Coin::Sei => 23149,
            Coin::Pyth => 28177,
            Coin::Silver => 28239,
            Coin::Dydx => 28324,
            Coin::Inj => 7226,
            Coin::StTia => 29427,
        }
    }

    pub(crate) fn all() -> [Coin; 3] {
        [Coin::Atom, Coin::Levana, Coin::Eth]
    }
}
