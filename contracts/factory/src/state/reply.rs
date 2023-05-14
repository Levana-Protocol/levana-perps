use cw_storage_plus::Item;
use msg::prelude::*;
use serde::{Deserialize, Serialize};
use shared::namespace;
use std::convert::TryFrom;

const INSTANTIATE_MARKET: Item<InstantiateMarket> = Item::new(namespace::REPLY_INSTANTIATE_MARKET);

#[derive(Serialize, Deserialize)]
pub struct InstantiateMarket {
    pub market_id: MarketId,
    pub migration_admin: Addr,
    pub price_admin: Addr,
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
pub enum ReplyId {
    InstantiateMarket = 0,
    InstantiatePositionToken = 1,
    InstantiateLiquidityTokenLp = 2,
    InstantiateLiquidityTokenXlp = 3,
}

impl TryFrom<u64> for ReplyId {
    type Error = anyhow::Error;

    fn try_from(value: u64) -> Result<Self> {
        match value {
            0 => Ok(ReplyId::InstantiateMarket),
            1 => Ok(ReplyId::InstantiatePositionToken),
            2 => Ok(ReplyId::InstantiateLiquidityTokenLp),
            3 => Ok(ReplyId::InstantiateLiquidityTokenXlp),
            _ => Err(anyhow!("{value} is not a valid reply id")),
        }
    }
}

impl From<ReplyId> for u64 {
    fn from(src: ReplyId) -> u64 {
        src as u64
    }
}
