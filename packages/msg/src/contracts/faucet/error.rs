use cosmwasm_std::{Addr, Decimal256};

#[derive(serde::Serialize)]
pub enum FaucetError {
    TooSoon { wait_secs: Decimal256 },
    AlreadyTapped { cw20: Addr },
}
