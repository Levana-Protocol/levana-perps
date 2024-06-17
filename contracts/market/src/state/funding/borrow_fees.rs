use cosmwasm_std::{Decimal256, OverflowError};

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
pub(crate) struct BorrowFees {
    pub(crate) lp: Decimal256,
    pub(crate) xlp: Decimal256,
}

impl BorrowFees {
    pub(crate) fn total(&self) -> Result<Decimal256, OverflowError> {
        self.lp.checked_add(self.xlp)
    }
}
