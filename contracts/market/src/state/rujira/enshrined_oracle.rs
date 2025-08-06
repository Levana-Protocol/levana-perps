use crate::state::rujira::grpc::{Queryable, QueryablePair};
use anyhow::{anyhow, Error};
use cosmwasm_std::{Decimal, QuerierWrapper};
use std::str::FromStr;

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryOraclePriceRequest {
    #[prost(string, tag = "1")]
    pub height: String,
    #[prost(string, tag = "2")]
    pub symbol: String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OraclePrice {
    #[prost(string, tag = "1")]
    pub symbol: String,
    #[prost(string, tag = "2")]
    pub price: String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryOraclePriceResponse {
    #[prost(message, optional, tag = "1")]
    pub price: Option<OraclePrice>,
}

pub struct EnshrinedPrice {
    pub price: Decimal,
}

impl TryFrom<OraclePrice> for EnshrinedPrice {
    type Error = Error;

    fn try_from(value: OraclePrice) -> Result<Self, Self::Error> {
        let dec = Decimal::from_str(&value.price).map_err(|e| {
            anyhow!(
                "Failed to parse price '{}' for OraclePrice: {}",
                value.price,
                e
            )
        })?;
        Ok(Self { price: dec })
    }
}

impl EnshrinedPrice {
    pub fn load(q: QuerierWrapper, symbol: String) -> Result<Self, Error> {
        let req = QueryOraclePriceRequest {
            height: "0".to_string(),
            symbol: symbol.clone(),
        };
        let res = QueryOraclePriceResponse::get(q, req)?;
        let asset = res.price.ok_or_else(|| {
            anyhow!(
                "No price returned from enshrined oracle for symbol: {}",
                symbol
            )
        })?;

        if asset.symbol != symbol {
            return Err(anyhow!(
                "Symbol mismatch: expected {}, found {}",
                symbol,
                asset.symbol
            ));
        }

        EnshrinedPrice::try_from(asset)
    }
}

impl QueryablePair for QueryOraclePriceResponse {
    type Request = QueryOraclePriceRequest;
    type Response = QueryOraclePriceResponse;

    fn grpc_path() -> &'static str {
        "/types.Query/OraclePrice"
    }
}
