//! Data types for representing the assets covered by a market.
use cosmwasm_std::{StdError, StdResult};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use schemars::{
    schema::{InstanceType, SchemaObject},
    JsonSchema,
};
use serde::de::Visitor;

use crate::prelude::*;

/// Whether the collateral asset is the same as the quote or base asset.
#[cw_serde]
#[derive(Eq, Hash, Copy)]
pub enum MarketType {
    /// A market where the collateral is the quote asset
    CollateralIsQuote,
    /// A market where the collateral is the base asset
    CollateralIsBase,
}

/// An identifier for a market.
#[derive(Clone)]
pub struct MarketId {
    base: String,
    quote: String,
    market_type: MarketType,
    encoded: String,
}

impl std::hash::Hash for MarketId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.encoded.hash(state);
    }
}

impl PartialEq for MarketId {
    fn eq(&self, other: &Self) -> bool {
        self.encoded == other.encoded
    }
}

impl Eq for MarketId {}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for MarketId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.encoded.partial_cmp(&other.encoded)
    }
}

impl Ord for MarketId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.encoded.cmp(&other.encoded)
    }
}

impl std::fmt::Debug for MarketId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.encoded, f)
    }
}

/// We hardcode a list of assets that are treated as fiat and therefore, when
/// used as quote, are by default assumed to not be collateral.
fn is_fiat(s: &str) -> bool {
    // can be expanded in the future
    s == "USD" || s == "EUR"
}

fn make_encoded(base: &str, quote: &str, market_type: MarketType) -> String {
    let (base_plus, quote_plus) = match (market_type, is_fiat(quote)) {
        // ATOM_USD but USD is quote and fiat, add the plus to override fiat default
        (MarketType::CollateralIsQuote, true) => ("", "+"),
        // ATOM_USDC and USDC is the quote, therefore default will be USDC as collateral, no plus
        (MarketType::CollateralIsQuote, false) => ("", ""),
        // ATOM_USD and ATOM should be the collateral, that's assumed, no plus needed
        (MarketType::CollateralIsBase, true) => ("", ""),
        // ATOM_USDC and ATOM should be the collateral, need to override with a plus
        (MarketType::CollateralIsBase, false) => ("+", ""),
    };
    format!("{base}{base_plus}_{quote}{quote_plus}")
}

impl MarketId {
    /// Construct a new [MarketId].
    pub fn new(base: impl Into<String>, quote: impl Into<String>, market_type: MarketType) -> Self {
        let base = base.into();
        let quote = quote.into();
        let encoded = make_encoded(&base, &quote, market_type);
        MarketId {
            base,
            quote,
            market_type,
            encoded,
        }
    }

    /// Is the notional asset USD?
    ///
    /// This is used to bypass some currency conversions when they aren't necessary.
    pub fn is_notional_usd(&self) -> bool {
        self.get_notional() == "USD"
    }

    /// Get the string representation of the market.
    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    fn parse(s: &str) -> Option<Self> {
        let (base, quote) = s.split_once('_')?;
        let (base, base_is_collateral) = match base.strip_suffix('+') {
            Some(base) => {
                if is_fiat(quote) {
                    return None;
                } else {
                    (base, true)
                }
            }
            None => (base, false),
        };
        let (quote, quote_is_collateral) = match quote.strip_suffix('+') {
            Some(quote) => {
                if is_fiat(quote) {
                    (quote, true)
                } else {
                    return None;
                }
            }
            None => (quote, false),
        };
        let market_type = match (base_is_collateral, quote_is_collateral) {
            (true, true) => return None,
            (true, false) => MarketType::CollateralIsBase,
            (false, true) => MarketType::CollateralIsQuote,
            (false, false) => {
                if is_fiat(quote) {
                    MarketType::CollateralIsBase
                } else {
                    MarketType::CollateralIsQuote
                }
            }
        };

        assert_eq!(make_encoded(base, quote, market_type), s);
        Some(MarketId {
            base: base.to_owned(),
            quote: quote.to_owned(),
            market_type,
            encoded: s.to_owned(),
        })
    }

    /// Get the notional currency.
    pub fn get_notional(&self) -> &str {
        match self.market_type {
            MarketType::CollateralIsQuote => &self.base,
            MarketType::CollateralIsBase => &self.quote,
        }
    }

    /// Get the collateral currency
    pub fn get_collateral(&self) -> &str {
        match self.market_type {
            MarketType::CollateralIsQuote => &self.quote,
            MarketType::CollateralIsBase => &self.base,
        }
    }

    /// Get the base currency
    pub fn get_base(&self) -> &str {
        &self.base
    }

    /// Get the quote currency
    pub fn get_quote(&self) -> &str {
        &self.quote
    }

    /// Determine the market type
    pub fn get_market_type(&self) -> MarketType {
        self.market_type
    }
}

impl Display for MarketId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.encoded)
    }
}

impl FromStr for MarketId {
    type Err = StdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MarketId::parse(s)
            .ok_or_else(|| StdError::parse_err("MarketId", format!("Invalid market ID: {s}")))
    }
}

impl serde::Serialize for MarketId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.encoded)
    }
}

impl<'de> serde::Deserialize<'de> for MarketId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(MarketIdVisitor)
    }
}

struct MarketIdVisitor;

impl<'de> Visitor<'de> for MarketIdVisitor {
    type Value = MarketId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("MarketId")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        MarketId::parse(v).ok_or_else(|| E::custom(format!("Invalid market ID: {v}")))
    }
}

impl JsonSchema for MarketId {
    fn schema_name() -> String {
        "MarketId".to_owned()
    }

    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            format: Some("market-id".to_owned()),
            ..Default::default()
        }
        .into()
    }
}

impl<'a> PrimaryKey<'a> for MarketId {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        let key = Key::Ref(self.encoded.as_bytes());

        vec![key]
    }
}

impl<'a> Prefixer<'a> for MarketId {
    fn prefix(&self) -> Vec<Key> {
        let key = Key::Ref(self.encoded.as_bytes());
        vec![key]
    }
}

impl KeyDeserialize for MarketId {
    type Output = MarketId;

    const KEY_ELEMS: u16 = 1;

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        std::str::from_utf8(&value)
            .map_err(StdError::invalid_utf8)
            .and_then(|s| s.parse())
    }
}

impl KeyDeserialize for &'_ MarketId {
    type Output = MarketId;

    const KEY_ELEMS: u16 = 1;

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        std::str::from_utf8(&value)
            .map_err(StdError::invalid_utf8)
            .and_then(|s| s.parse())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_usd_as_collateral() {
        let orig = MarketId::new("BTC", "USD", MarketType::CollateralIsQuote);
        assert_eq!(orig.as_str(), "BTC_USD+");
        let parsed: MarketId = "BTC_USD+".parse().unwrap();
        assert_eq!(orig, parsed);

        assert_eq!(orig.get_base(), "BTC");
        assert_eq!(orig.get_quote(), "USD");
        assert_eq!(orig.get_notional(), "BTC");
        assert_eq!(orig.get_collateral(), "USD");
    }

    #[test]
    fn round_trip_usd_as_notional() {
        let orig = MarketId::new("BTC", "USD", MarketType::CollateralIsBase);
        assert_eq!(orig.as_str(), "BTC_USD");
        let parsed: MarketId = "BTC_USD".parse().unwrap();
        assert_eq!(orig, parsed);

        assert_eq!(orig.get_base(), "BTC");
        assert_eq!(orig.get_quote(), "USD");
        assert_eq!(orig.get_notional(), "USD");
        assert_eq!(orig.get_collateral(), "BTC");
    }

    #[test]
    fn round_trip_usdc_as_collateral() {
        let orig = MarketId::new("BTC", "USDC", MarketType::CollateralIsQuote);
        assert_eq!(orig.as_str(), "BTC_USDC");
        let parsed: MarketId = "BTC_USDC".parse().unwrap();
        assert_eq!(orig, parsed);

        assert_eq!(orig.get_base(), "BTC");
        assert_eq!(orig.get_quote(), "USDC");
        assert_eq!(orig.get_notional(), "BTC");
        assert_eq!(orig.get_collateral(), "USDC");
    }

    #[test]
    fn round_trip_usdc_as_notional() {
        let orig = MarketId::new("BTC", "USDC", MarketType::CollateralIsBase);
        assert_eq!(orig.as_str(), "BTC+_USDC");
        let parsed: MarketId = "BTC+_USDC".parse().unwrap();
        assert_eq!(orig, parsed);

        assert_eq!(orig.get_base(), "BTC");
        assert_eq!(orig.get_quote(), "USDC");
        assert_eq!(orig.get_notional(), "USDC");
        assert_eq!(orig.get_collateral(), "BTC");
    }

    #[test]
    fn no_unnecessary_plus() {
        MarketId::from_str("BTC+_USD").unwrap_err();
        MarketId::from_str("BTC_USDC+").unwrap_err();
        assert_eq!(
            MarketId::from_str("BTC_USD").unwrap(),
            MarketId::new("BTC", "USD", MarketType::CollateralIsBase)
        );
        assert_eq!(
            MarketId::from_str("BTC_USD+").unwrap(),
            MarketId::new("BTC", "USD", MarketType::CollateralIsQuote)
        );
        assert_eq!(
            MarketId::from_str("BTC_USDC").unwrap(),
            MarketId::new("BTC", "USDC", MarketType::CollateralIsQuote)
        );
        assert_eq!(
            MarketId::from_str("BTC+_USDC").unwrap(),
            MarketId::new("BTC", "USDC", MarketType::CollateralIsBase)
        );
    }

    #[test]
    fn round_trip_market_id_collateral_is_quote() {
        let orig = MarketId::new("BTC", "USD", MarketType::CollateralIsQuote);
        assert_eq!(orig.as_str(), "BTC_USD+");
        let parsed: MarketId = "BTC_USD+".parse().unwrap();
        assert_eq!(orig, parsed);

        assert_eq!(orig.get_base(), "BTC");
        assert_eq!(orig.get_quote(), "USD");
        assert_eq!(orig.get_notional(), "BTC");
        assert_eq!(orig.get_collateral(), "USD");
    }

    #[test]
    fn round_trip_market_id_usd_collateral_is_base() {
        let orig = MarketId::new("BTC", "USD", MarketType::CollateralIsBase);
        assert_eq!(orig.as_str(), "BTC_USD");
        let parsed: MarketId = "BTC_USD".parse().unwrap();
        assert_eq!(orig, parsed);

        assert_eq!(orig.get_base(), "BTC");
        assert_eq!(orig.get_quote(), "USD");
        assert_eq!(orig.get_notional(), "USD");
        assert_eq!(orig.get_collateral(), "BTC");
    }

    #[test]
    fn notional_to_usd() {
        // Assume ATOM is notional/base, OSMO is collateral/quote
        let market_id = MarketId::from_str("ATOM_OSMO").unwrap();

        // $2 per OSMO
        let price_usd = PriceCollateralInUsd::from_str("2").unwrap();

        // And 5 OSMO to one ATOM, e.g. $10 per ATOM
        let price_base = PriceBaseInQuote::from_str("5").unwrap();

        let price_point = PricePoint {
            price_notional: price_base.into_notional_price(market_id.get_market_type()),
            price_usd,
            price_base,
            timestamp: Default::default(),
            is_notional_usd: market_id.is_notional_usd(),
            market_type: market_id.get_market_type(),
            publish_time: None,
            publish_time_usd: None,
        };

        let in_usd = price_point.notional_to_usd("50".parse().unwrap());

        // 50 ATOM should be $500
        assert_eq!(in_usd, "500".parse().unwrap());
    }
}
