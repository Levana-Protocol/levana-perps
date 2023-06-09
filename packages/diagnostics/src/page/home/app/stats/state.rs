use std::collections::{HashSet, VecDeque};

use crate::{
    page::home::app::{ActionHistory, TxEvent},
    prelude::*,
};
use cosmwasm_std::Decimal256;
use msg::contracts::market::{
    entry::{QueryMsg, StatusResp},
    position::PositionId,
};
use wasm_bindgen_futures::spawn_local;

pub struct Stats {
    pub position_ids: Mutable<HashSet<PositionId>>,
    pub deposit_collateral: Mutable<Signed<Collateral>>,
    pub trade_volume: Mutable<Usd>,
    pub realized_pnl: Mutable<Signed<Usd>>,
    pub price: Mutable<Option<PriceBaseInQuote>>,
    pub market_status: Mutable<Option<StatusResp>>,
    pub action_history: MutableVec<ActionHistory>,
}

impl Stats {
    pub fn new(bridge: Rc<Bridge>) -> Rc<Self> {
        // a _little_ dirty race condition, but, meh
        let price = Mutable::new(None);
        let market_status = Mutable::new(None);
        spawn_local(clone!(price, bridge, market_status => async move {
            let resp = bridge.query_market::<PricePoint>(QueryMsg::SpotPrice{timestamp: None}).await.unwrap();
            price.set(Some(resp.data.price_base));
            let resp = bridge.query_market::<StatusResp>(QueryMsg::Status{price:None}).await.unwrap();
            market_status.set(Some(resp.data));
        }));

        Rc::new(Self {
            position_ids: Mutable::new(HashSet::new()),
            deposit_collateral: Mutable::new(Signed::<Collateral>::zero()),
            trade_volume: Mutable::new(Usd::zero()),
            realized_pnl: Mutable::new(Signed::<Usd>::zero()),
            price,
            market_status,
            action_history: MutableVec::new(),
        })
    }
}

pub trait NumberExt {
    fn into_number_ext(&self) -> Number;
}

pub trait NumberSliceExt {
    fn perc(&self, value: Number, min: Option<Number>) -> Option<Number>;
}

impl<T: NumberExt> NumberSliceExt for &[T] {
    fn perc(&self, value: Number, min: Option<Number>) -> Option<Number> {
        let min = match min {
            None => match self.iter().map(|x| x.into_number_ext()).min() {
                None => return None,
                Some(min) => min,
            },
            Some(min) => min,
        };

        let max = self.iter().map(|x| x.into_number_ext()).max();

        max.and_then(|max| {
            let denom = max - min;

            if denom == Number::ZERO {
                return None;
            } else {
                Some((value - min) / denom)
            }
        })
    }
}

impl NumberExt for usize {
    fn into_number_ext(&self) -> Number {
        Number::from(u64::try_from(*self).unwrap())
    }
}

impl NumberExt for Signed<Collateral> {
    fn into_number_ext(&self) -> Number {
        self.into_number()
    }
}

impl NumberExt for Signed<Decimal256> {
    fn into_number_ext(&self) -> Number {
        self.into_number()
    }
}

impl NumberExt for Collateral {
    fn into_number_ext(&self) -> Number {
        self.into_number()
    }
}

impl NumberExt for Usd {
    fn into_number_ext(&self) -> Number {
        self.into_number()
    }
}

impl NumberExt for Signed<Usd> {
    fn into_number_ext(&self) -> Number {
        self.into_number()
    }
}

impl NumberExt for PriceBaseInQuote {
    fn into_number_ext(&self) -> Number {
        self.into_number()
    }
}

impl NumberExt for Notional {
    fn into_number_ext(&self) -> Number {
        self.into_number()
    }
}
