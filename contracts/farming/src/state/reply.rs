use crate::prelude::*;
use std::convert::TryFrom;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum ReplyId {
    TransferCollateral = 0,
    ReinvestYield = 1,
    FarmingDepositXlp = 2,
}

/// This represents the portion of the yield that is allocated to the Bonus Fund during the
/// process of reinvesting yield.
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

/// The address of the farmer who deposited Collateral.
pub(crate) struct ReplyFarmerAddr;

impl ReplyFarmerAddr {
    const FARMER_ADDR: Item<'static, Addr> = Item::new(namespace::REPLY_FARMER_ADDR);

    pub(crate) fn load(store: &dyn Storage) -> Result<Addr> {
        Self::FARMER_ADDR.load(store).map_err(|err| err.into())
    }

    pub(crate) fn save(store: &mut dyn Storage, farmer_addr: Option<&Addr>) -> Result<()> {
        match farmer_addr {
            None => {
                Self::FARMER_ADDR.remove(store);
                Ok(())
            }
            Some(addr) => Self::FARMER_ADDR
                .save(store, addr)
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
            2 => Ok(ReplyId::FarmingDepositXlp),
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
        src as u64
    }
}
