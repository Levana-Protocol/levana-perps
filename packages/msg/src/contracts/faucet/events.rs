use super::{entry::FaucetAsset, error::FaucetError};
use cosmwasm_std::Addr;
use shared::prelude::*;

pub struct TapEvent {
    pub recipient: Addr,
    pub amount: Number,
    pub asset: FaucetAsset,
}

impl PerpEvent for TapEvent {}
impl From<TapEvent> for cosmwasm_std::Event {
    fn from(src: TapEvent) -> Self {
        let evt = cosmwasm_std::Event::new("faucet-tap").add_attributes(vec![
            ("recipient", src.recipient.to_string()),
            ("amount", src.amount.to_string()),
        ]);

        match src.asset {
            FaucetAsset::Cw20(addr) => {
                evt.add_attributes(vec![("asset-kind", "cw20"), ("asset-addr", addr.as_str())])
            }
            FaucetAsset::Native(denom) => {
                evt.add_attributes(vec![("asset-kind", "native"), ("asset-denom", &denom)])
            }
        }
    }
}

/// The event name for this will be the address itself
/// due to backwards compatibility requirements
pub struct FaucetErrorEvent {
    pub addr: Addr,
    pub error: FaucetError,
}
impl PerpEvent for FaucetErrorEvent {}

impl From<FaucetErrorEvent> for cosmwasm_std::Event {
    fn from(src: FaucetErrorEvent) -> Self {
        match src.error {
            FaucetError::TooSoon { wait_secs } => {
                Event::new(src.addr).add_attribute("wait_secs", wait_secs.to_string())
            }
            FaucetError::AlreadyTapped { cw20 } => {
                Event::new(src.addr).add_attribute("already_tapped", cw20.into_string())
            }
        }
    }
}

/// The event name for this will be the address itself
/// due to backwards compatibility requirements
pub struct FaucetSuccessEvent {
    pub addr: Addr,
}
impl PerpEvent for FaucetSuccessEvent {}

impl From<FaucetSuccessEvent> for cosmwasm_std::Event {
    fn from(src: FaucetSuccessEvent) -> Self {
        Event::new(src.addr).add_attribute("success", "success")
    }
}
