use cosmos::Address;
use msg::contracts::market::position::PositionId;
use sqlx::FromRow;

use crate::endpoints::pnl::PnlType;

#[derive(FromRow, Debug, PartialEq, Eq)]
pub(crate) struct AddressModel {
    pub(crate) id: i64,
    pub(crate) address: String,
}

#[derive(FromRow, Debug, PartialEq, Eq)]
pub(crate) struct PositionDetail {
    pub(crate) id: i64,
    pub(crate) contract_address: i64,
    pub(crate) chain: String,
    pub(crate) position_id: i64,
    pub(crate) pnl_type: String,
    pub(crate) url_id: i32,
}

#[derive(FromRow, Debug, PartialEq, Eq)]
pub(crate) struct NewPositionDetail {
    pub(crate) chain: String,
    pub(crate) address: String,
    pub(crate) position_id: i64,
}

// This is a type safe variant of PositionDetail with more
// information.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UrlDetail {
    pub(crate) id: i64,
    pub(crate) contract_address: Address,
    pub(crate) chain: String,
    pub(crate) position_id: PositionId,
    pub(crate) pnl_type: PnlType,
    pub(crate) url_id: i32,
}
