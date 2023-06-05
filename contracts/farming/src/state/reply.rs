use crate::prelude::*;
use std::convert::TryFrom;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum ReplyId {
    TransferCollateral = 0,
}

impl TryFrom<u64> for ReplyId {
    type Error = PerpError<u64>;

    fn try_from(value: u64) -> Result<Self, PerpError<u64>> {
        match value {
            0 => Ok(ReplyId::TransferCollateral),
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
