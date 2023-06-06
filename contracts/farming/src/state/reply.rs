use crate::prelude::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::convert::TryFrom;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum ReplyId {
    TransferCollateral = 0,
    ReinvestYield = 1,
    FarmingDepositXlp = 2,
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

/// Sometimes it's necessary to access certain data in a reply handler that existed in the
/// execution handler that sent the original SubMsg but is not persisted.
/// `EphemeralReplyData` provides a solution to this problem by allowing the original execution
/// handler to store data that would otherwise not be persisted and for the reply handler to load it
/// as needed. Once loaded, the data automatically is cleaned up.
pub(crate) struct EphemeralReplyData<'a, T> {
    item: Item<'a, T>,
}

impl<T> EphemeralReplyData<'_, T>
where
    T: Serialize + DeserializeOwned,
{
    /// Loads an item and then remove it
    pub(crate) fn load_once(&self, store: &mut dyn Storage) -> Result<T> {
        let data = self.item.load(store)?;
        self.item.remove(store);

        Ok(data)
    }

    /// Save an item
    pub(crate) fn save(&self, store: &mut dyn Storage, data: &T) -> Result<()> {
        self.item.save(store, data).map_err(|err| err.into())
    }
}

/// The portion of yield that was earned by the Farming Contract that is allocated to the bonus fund
/// and is not reinvested into xLP.
pub(crate) const EPHEMERAL_BONUS_FUND: EphemeralReplyData<Collateral> = EphemeralReplyData {
    item: Item::new(namespace::EPHEMERAL_BONUS_FUND),
};

/// The address of the farmer who sent the [ExecuteMsg::Deposit] msg with Collateral instead of xLP.
pub(crate) const EPHEMERAL_FARMER_ADDR: EphemeralReplyData<Addr> = EphemeralReplyData {
    item: Item::new(namespace::EPHEMERAL_FARMER_ADDR),
};
