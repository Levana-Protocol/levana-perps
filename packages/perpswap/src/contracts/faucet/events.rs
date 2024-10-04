use super::entry::FaucetAsset;
use crate::prelude::*;
use cosmwasm_std::Addr;

pub struct TapEvent {
    pub recipient: Addr,
    pub amount: Number,
    pub asset: FaucetAsset,
}

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
