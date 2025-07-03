use anyhow::Error;
use cosmwasm_std::{Binary, QuerierWrapper};
use prost::Message;

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
