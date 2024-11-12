use cw_storage_plus::Item;
use perpswap::namespace;
use perpswap::prelude::*;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

const INSTANTIATE_MARKET: Item<InstantiateMarket> = Item::new(namespace::REPLY_INSTANTIATE_MARKET);

pub(crate) const INSTANTIATE_COPY_TRADING: Item<InstantiateCopyTrading> =
    Item::new(namespace::REPLY_INSTANTIATE_COPY_TRADING);

#[derive(Serialize, Deserialize)]
pub(crate) struct InstantiateMarket {
    pub(crate) market_id: MarketId,
    pub(crate) migration_admin: Addr,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct InstantiateCopyTrading {
    pub(crate) migration_admin: Addr,
    pub(crate) leader: Addr,
}

pub(crate) fn reply_get_instantiate_market(store: &dyn Storage) -> Result<InstantiateMarket> {
    INSTANTIATE_MARKET.load(store).map_err(|err| err.into())
}

pub(crate) fn reply_set_instantiate_market(
    store: &mut dyn Storage,
    data: InstantiateMarket,
) -> Result<()> {
    INSTANTIATE_MARKET
        .save(store, &data)
        .map_err(|err| err.into())
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
pub(crate) enum ReplyId {
    InstantiateMarket = 0,
    InstantiatePositionToken = 1,
    InstantiateLiquidityTokenLp = 2,
    InstantiateLiquidityTokenXlp = 3,
    InstantiateCopyTrading = 4,
}

impl TryFrom<u64> for ReplyId {
    type Error = PerpError<u64>;

    fn try_from(value: u64) -> Result<Self, PerpError<u64>> {
        match value {
            0 => Ok(ReplyId::InstantiateMarket),
            1 => Ok(ReplyId::InstantiatePositionToken),
            2 => Ok(ReplyId::InstantiateLiquidityTokenLp),
            3 => Ok(ReplyId::InstantiateLiquidityTokenXlp),
            4 => Ok(ReplyId::InstantiateCopyTrading),
            _ => Err(PerpError {
                id: ErrorId::InternalReply,
                domain: ErrorDomain::Factory,
                description: format!("{value} is not a valid reply id"),
                data: Some(value),
            }),
        }
    }
}

impl From<ReplyId> for u64 {
    fn from(value: ReplyId) -> Self {
        match value {
            ReplyId::InstantiateMarket => 0,
            ReplyId::InstantiatePositionToken => 1,
            ReplyId::InstantiateLiquidityTokenLp => 2,
            ReplyId::InstantiateLiquidityTokenXlp => 3,
            ReplyId::InstantiateCopyTrading => 4,
        }
    }
}
