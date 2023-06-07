use crate::prelude::*;
use std::convert::TryFrom;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum ReplyId {
    TransferCollateral = 0,
    ReinvestYield = 1,
}

pub(crate) struct ReplyExpectedYield;

impl ReplyExpectedYield {
    const EXPECTED_YIELD: Item<'static, Collateral> = Item::new(namespace::REPLY_EXPECTED_YIELD);

    pub(crate) fn load(store: &dyn Storage) -> Result<Collateral> {
        Self::EXPECTED_YIELD.load(store).map_err(|err| err.into())
    }

    pub(crate) fn save(store: &mut dyn Storage, expected_yield: Option<Collateral>) -> Result<()> {
        match expected_yield {
            None => {
                Self::EXPECTED_YIELD.remove(store);
                Ok(())
            }
            Some(expected_yield) => Self::EXPECTED_YIELD
                .save(store, &expected_yield)
                .map_err(|err| err.into()),
        }
    }
}

impl TryFrom<u64> for ReplyId {
    type Error = PerpError<u64>;

    fn try_from(value: u64) -> Result<Self, PerpError<u64>> {
        match value {
            0 => Ok(ReplyId::TransferCollateral),
            1 => Ok(ReplyId::ReinvestYield),
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
    fn from(src: ReplyId) -> u64 {
        // SAFE: due to repr(u64)
        src as u64
    }
}
