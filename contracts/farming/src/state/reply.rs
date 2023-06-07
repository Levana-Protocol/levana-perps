use crate::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum ReplyId {
    TransferCollateral = 0,
    ReinvestYield = 1,
    FarmingDeposit = 2,
}

impl TryFrom<u64> for ReplyId {
    type Error = PerpError<u64>;

    fn try_from(value: u64) -> Result<Self, PerpError<u64>> {
        match value {
            0 => Ok(ReplyId::TransferCollateral),
            1 => Ok(ReplyId::ReinvestYield),
            2 => Ok(ReplyId::FarmingDeposit),
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

/// Sometimes it's necessary for a reply handler to access certain data that only existed in the
/// execution handler that sent the original SubMsg.
/// `EphemeralReplyData` provides a solution to this problem by allowing the original execution
/// handler to store data that would otherwise not be persisted, and for the reply handler to load it
/// as needed. Once loaded, the data is automatically cleaned up.
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
        debug_assert!(self.item.may_load(store)?.is_none());
        self.item.save(store, data).map_err(|err| err.into())
    }
}

/// The portion of yield that was earned by the Farming Contract that is allocated to the bonus fund
/// and is not reinvested into xLP.
pub(crate) const EPHEMERAL_BONUS_FUND: EphemeralReplyData<Collateral> = EphemeralReplyData {
    item: Item::new(namespace::EPHEMERAL_BONUS_FUND),
};

#[derive(Serialize, Deserialize)]
pub(crate) struct DepositReplyData {
    /// The address of the farmer who deposited Collateral or LP
    pub(crate) farmer: Addr,
    /// The xLP balance before the farming contract sends the Collateral or LP
    pub(crate) xlp_balance_before: LpToken,
}

/// The address of the farmer who sent the [ExecuteMsg::Deposit] msg with Collateral instead of xLP.
pub(crate) const EPHEMERAL_DEPOSIT_COLLATERAL_DATA: EphemeralReplyData<DepositReplyData> =
    EphemeralReplyData {
        item: Item::new(namespace::EPHEMERAL_DEPOSIT_COLLATERAL_DATA),
    };
