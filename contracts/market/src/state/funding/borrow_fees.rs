use cosmwasm_std::Decimal256;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
pub(crate) struct BorrowFees {
    pub(crate) lp: Decimal256,
    pub(crate) xlp: Decimal256,
}

impl BorrowFees {
    pub(crate) fn total(&self) -> Decimal256 {
        self.lp + self.xlp
    }
}
