use anyhow::Error;
use cosmwasm_std::{Binary, Decimal, QuerierWrapper, Uint128};
use prost::Message;
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

impl Network {
    pub fn load(q: QuerierWrapper) -> Result<Self, Error> {
        let req = QueryNetworkRequest {
            height: "0".to_string(),
        };
        let res = QueryNetworkResponse::get(q, req)?;
        Network::try_from(res)
    }
}

pub trait QueryablePair {
    type Request: Message + Default;
    type Response: Message + Sized + Default;

    fn grpc_path() -> &'static str;
}

pub trait Queryable: Sized {
    type Pair: QueryablePair;

    fn get(
        querier: QuerierWrapper,
        req: <Self::Pair as QueryablePair>::Request,
    ) -> Result<Self, Error>;
}

impl<T> Queryable for T
where
    T: QueryablePair<Response = Self> + Message + Default,
{
    type Pair = T;

    fn get(
        querier: QuerierWrapper,
        req: <Self::Pair as QueryablePair>::Request,
    ) -> Result<Self, Error> {
        let mut buf = Vec::new();
        req.encode(&mut buf)?;
        let res = querier
            .query_grpc(Self::grpc_path().to_string(), Binary::from(buf))?
            .to_vec();
        Ok(Self::decode(&*res)?)
    }
}

impl QueryablePair for QueryNetworkResponse {
    type Request = QueryNetworkRequest;
    type Response = QueryNetworkResponse;

    fn grpc_path() -> &'static str {
        "/types.Query/Network"
    }
}
