use std::str::FromStr;

use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize, Clone)]
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

#[derive(Debug, serde::Serialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CmcMarketPair {
    pub(crate) exchange_id: ExchangeId,
    pub(crate) exchange_name: String,
    pub(crate) market_id: String,
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
    /// Best way to determine if an exchange is CEX or DEX, is to go
    /// to the cryptocurrency page of the CMC and try finding the
    /// markets. Then based on the CEX or DEX filter, you can find the
    /// exchange type.

    /// Another way to determine if through the coingecko page. Eg:
    /// https://www.coingecko.com/en/exchanges/okx
    ///
    /// Unfortunately, CMC doesn't provide an API for this currently.
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
            | 5319 | 6137 | 7748 | 8080 | 8767 | 9217 | 9384 | 8388 | 9298 | 9452 | 70 | 657
            | 699 | 1131 | 1336 | 9015 | 9108 | 9586 | 5751 | 335 | 890 | 1011 | 1161 | 1234
            | 219 | 561 | 1117 | 1250 | 1254 | 9329 | 224 | 257 | 360 | 385 | 460 | 479 | 567
            | 741 | 908 | 1029 | 1138 | 1295 | 1379 | 1411 | 1539 | 1600 | 1601 | 7500 | 8617
            | 8701 | 9173 | 9717 | 213 | 261 | 422 | 491 | 498 | 564 | 587 | 599 | 603 | 636
            | 653 | 717 | 731 | 826 | 863 | 868 | 961 | 1128 | 1524 | 1628 | 1679 | 5344 | 7421
            | 34 | 100 | 127 | 166 | 201 | 209 | 223 | 228 | 248 | 252 | 363 | 419 | 480 | 514
            | 843 | 844 | 945 | 9613 | 73 | 80 | 705 | 1173 | 258 | 483 | 644 | 655 | 1071
            | 1091 | 61 | 106 | 139 | 171 | 250 | 280 | 321 | 364 | 1371 | 369 | 997 | 96
            | 9588 | 922 | 925 | 137 | 1037 | 7893 | 605 | 9798 | 9867 | 633 | 5750 | 7680
            | 9957 => Ok(ExchangeKind::Cex),
            1707 | 1454 | 1187 | 1530 | 1567 | 1344 | 1714 | 9244 | 1165 | 1293 | 1327 | 1395
            | 1447 | 1547 | 1551 | 1614 | 1657 | 1665 | 6255 | 6444 | 6706 | 6757 | 8915 | 9245
            | 1342 | 1426 | 1612 | 8161 | 1069 | 246 | 267 | 1062 | 1063 | 1070 | 1141 | 1206
            | 1209 | 1262 | 1297 | 1304 | 1333 | 1340 | 1348 | 1356 | 1359 | 1386 | 1404 | 1408
            | 1416 | 1417 | 1419 | 1440 | 1444 | 1455 | 1462 | 1463 | 1466 | 1469 | 1477 | 1478
            | 1484 | 1493 | 1503 | 1505 | 1510 | 1517 | 1521 | 1526 | 1541 | 1545 | 1554 | 1565
            | 1566 | 1570 | 1572 | 1581 | 1582 | 1585 | 1594 | 1604 | 1605 | 1625 | 1637 | 1640
            | 1646 | 1651 | 1668 | 1671 | 1708 | 1723 | 1922 | 2373 | 3177 | 4098 | 4191 | 4192
            | 5121 | 5558 | 5631 | 5876 | 6108 | 6506 | 6669 | 6707 | 6732 | 7042 | 7066 | 7070
            | 7071 | 7393 | 7475 | 7495 | 7516 | 8001 | 8002 | 1495 | 1546 | 1606 | 1688 | 1695
            | 1704 | 3874 | 5101 | 5116 | 5132 | 5118 | 5455 | 6799 | 5194 | 1464 | 6713 | 6728
            | 6745 | 7490 | 7930 | 6394 | 6668 | 1261 | 1428 | 1441 | 1445 | 1483 | 1598 | 5803
            | 6007 | 6663 | 6813 | 7313 | 1235 | 1302 | 1319 | 1329 | 3596 | 6407 | 7084 | 1279
            | 1398 | 1496 | 1528 | 1580 | 756 | 1101 | 1190 | 1310 | 1314 | 1418 | 1456 | 1692
            | 1699 | 5327 | 6420 | 6753 | 7392 | 503 | 7086 | 8913 | 9243 | 249 | 310 | 856
            | 983 | 1232 | 1281 | 1370 | 1378 | 1407 | 1413 | 1457 | 1480 | 1487 | 1489 | 1514
            | 1515 | 1584 | 1599 | 1685 | 1931 | 5430 | 7440 | 8003 | 8877 | 9882 | 9883 => {
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

        Ok(CmcMarketPair {
            exchange_id: ExchangeId(result.exchange.id),
            exchange_name: result.exchange.name,
            market_id: result.market_pair,
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
    Wif,
    Pepe,
    Bonk,
    Shib,
    Floki,
    Meme,
    Dot,
    Rune,
    Ntrn,
    Eur,
    StOsmo,
    Axl,
    Tia,
    /// Akash
    Akt,
    /// Secret network
    Scrt,
    RyEth,
    AxlEth,
    StkAtom,
    StDym,
    MilkTia,
}

impl FromStr for Coin {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ATOM" => Ok(Coin::Atom),
            "LEVANA" => Ok(Coin::Levana),
            "ETH" => Ok(Coin::Eth),
            "DOGE" => Ok(Coin::Dogecoin),
            "wBTC" => Ok(Coin::Wbtc),
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
            "WIF" => Ok(Coin::Wif),
            "PEPE" => Ok(Coin::Pepe),
            "BONK" => Ok(Coin::Bonk),
            "SHIB" => Ok(Coin::Shib),
            "FLOKI" => Ok(Coin::Floki),
            "MEME" => Ok(Coin::Meme),
            "RUNE" => Ok(Coin::Rune),
            "NTRN" => Ok(Coin::Ntrn),
            "EUR" => Ok(Coin::Eur),
            "stOSMO" => Ok(Coin::StOsmo),
            "AXL" => Ok(Coin::Axl),
            "TIA" => Ok(Coin::Tia),
            "AKT" => Ok(Coin::Akt),
            "SCRT" => Ok(Coin::Scrt),
            "ryETH" => Ok(Coin::RyEth),
            "axlETH" => Ok(Coin::AxlEth),
            "stkATOM" => Ok(Coin::StkAtom),
            "stDYM" => Ok(Coin::StDym),
            "milkTIA" => Ok(Coin::MilkTia),
            other => Err(anyhow!("Unsupported coin {other}")),
        }
    }
}

const BTC_CMC_ID: u32 = 1;
const ATOM_CMC_ID: u32 = 3794;
const DYDX_CMC_ID: u32 = 28324;
const ETH_CMC_ID: u32 = 1027;
const TIA_CMC_ID: u32 = 22861;
const OSMO_CMC_ID: u32 = 12220;
const DYM_CMC_ID: u32 = 28932;

impl Coin {
    pub(crate) fn cmc_id(&self) -> u32 {
        match self {
            // https://coinmarketcap.com/api/documentation/v1/#operation/getV1CryptocurrencyMap
            Coin::Atom => ATOM_CMC_ID,
            Coin::Levana => 28660,
            Coin::Eth => ETH_CMC_ID,
            Coin::Dogecoin => 74,
            Coin::Wbtc => BTC_CMC_ID,
            Coin::Avax => 5805,
            Coin::Btc => BTC_CMC_ID,
            Coin::StAtom => ATOM_CMC_ID,
            Coin::StDYDX => DYDX_CMC_ID,
            Coin::Bnb => 1839,
            Coin::Luna => 20314,
            Coin::Dym => DYM_CMC_ID,
            Coin::Osmo => OSMO_CMC_ID,
            Coin::Link => 1975,
            Coin::Sol => 5426,
            Coin::Sei => 23149,
            Coin::Pyth => 28177,
            Coin::Silver => 28239,
            Coin::Dydx => DYDX_CMC_ID,
            Coin::Inj => 7226,
            Coin::StTia => TIA_CMC_ID,
            Coin::Wif => 28752,
            Coin::Pepe => 24478,
            Coin::Bonk => 23095,
            Coin::Shib => 5994,
            Coin::Floki => 10804,
            Coin::Meme => 28301,
            Coin::Dot => 6636,
            Coin::Rune => 4157,
            Coin::Ntrn => 26680,
            Coin::Eur => 2790,
            Coin::StOsmo => OSMO_CMC_ID,
            Coin::Axl => 17799,
            Coin::Tia => TIA_CMC_ID,
            Coin::Akt => 7431,
            Coin::Scrt => 5604,
            Coin::RyEth => ETH_CMC_ID,
            Coin::AxlEth => ETH_CMC_ID,
            Coin::StkAtom => ATOM_CMC_ID,
            Coin::StDym => DYM_CMC_ID,
            Coin::MilkTia => TIA_CMC_ID,
        }
    }

    pub(crate) fn to_wrapped_coin(self) -> WrappedCoin {
        match self {
            Coin::Atom => WrappedCoin(Coin::Atom),
            Coin::Levana => WrappedCoin(Coin::Levana),
            Coin::Eth => WrappedCoin(Coin::Eth),
            Coin::Dogecoin => WrappedCoin(Coin::Dogecoin),
            Coin::Wbtc => WrappedCoin(Coin::Btc),
            Coin::Avax => WrappedCoin(Coin::Avax),
            Coin::Btc => WrappedCoin(Coin::Btc),
            Coin::StAtom => WrappedCoin(Coin::Atom),
            Coin::StDYDX => WrappedCoin(Coin::Dydx),
            Coin::Bnb => WrappedCoin(Coin::Bnb),
            Coin::Luna => WrappedCoin(Coin::Luna),
            Coin::Dym => WrappedCoin(Coin::Dym),
            Coin::Osmo => WrappedCoin(Coin::Osmo),
            Coin::Link => WrappedCoin(Coin::Link),
            Coin::Sol => WrappedCoin(Coin::Sol),
            Coin::Sei => WrappedCoin(Coin::Sei),
            Coin::Pyth => WrappedCoin(Coin::Pyth),
            Coin::Silver => WrappedCoin(Coin::Silver),
            Coin::Dydx => WrappedCoin(Coin::Dydx),
            Coin::Inj => WrappedCoin(Coin::Inj),
            Coin::StTia => WrappedCoin(Coin::Tia),
            Coin::Wif => WrappedCoin(Coin::Wif),
            Coin::Pepe => WrappedCoin(Coin::Pepe),
            Coin::Bonk => WrappedCoin(Coin::Bonk),
            Coin::Shib => WrappedCoin(Coin::Shib),
            Coin::Floki => WrappedCoin(Coin::Floki),
            Coin::Meme => WrappedCoin(Coin::Meme),
            Coin::Dot => WrappedCoin(Coin::Dot),
            Coin::Rune => WrappedCoin(Coin::Rune),
            Coin::Ntrn => WrappedCoin(Coin::Ntrn),
            Coin::Eur => WrappedCoin(Coin::Eur),
            Coin::StOsmo => WrappedCoin(Coin::Osmo),
            Coin::Axl => WrappedCoin(Coin::Axl),
            Coin::Tia => WrappedCoin(Coin::Tia),
            Coin::Akt => WrappedCoin(Coin::Akt),
            Coin::Scrt => WrappedCoin(Coin::Scrt),
            Coin::RyEth => WrappedCoin(Coin::Eth),
            Coin::AxlEth => WrappedCoin(Coin::Eth),
            Coin::StkAtom => WrappedCoin(Coin::Atom),
            Coin::StDym => WrappedCoin(Coin::Dym),
            Coin::MilkTia => WrappedCoin(Coin::Tia),
        }
    }
}

/// WrappedCoin is an abstraction over Coin that is used to decide
/// which coin is actually used for the computation of DNF.  For cases
/// like stOSMO, we use OSMO to decide the DNF and we use WrappedCoin
/// to differentiate it.
pub(crate) struct WrappedCoin(pub(crate) Coin);

impl From<&Coin> for WrappedCoin {
    fn from(value: &Coin) -> Self {
        WrappedCoin(*value)
    }
}
