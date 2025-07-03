use crate::state::rujira::grpc::{Queryable, QueryablePair};
use anyhow::Error;
use cosmwasm_std::{Decimal, QuerierWrapper, Uint128};
use std::{ops::Div, str::FromStr};

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryNetworkRequest {
    #[prost(string, tag = "1")]
    pub height: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryNetworkResponse {
    #[prost(string, tag = "13")]
    pub rune_price_in_tor: ::prost::alloc::string::String,
}

#[allow(dead_code)]
pub struct Network {
    pub rune_price_in_tor: Decimal,
}

impl TryFrom<QueryNetworkResponse> for Network {
    type Error = Error;

    fn try_from(value: QueryNetworkResponse) -> Result<Self, Self::Error> {
        let price = Decimal::from_str(&value.rune_price_in_tor)?.div(Uint128::from(10u128).pow(8));
        Ok(Network {
            rune_price_in_tor: price,
        })
    }
}

#[allow(dead_code)]
impl Network {
    pub fn load(q: QuerierWrapper) -> Result<Self, Error> {
        let req = QueryNetworkRequest {
            height: "0".to_string(),
        };
        let res = QueryNetworkResponse::get(q, req)?;
        Network::try_from(res)
    }
}

impl QueryablePair for QueryNetworkResponse {
    type Request = QueryNetworkRequest;
    type Response = QueryNetworkResponse;

    fn grpc_path() -> &'static str {
        "/types.Query/Network"
    }
}
