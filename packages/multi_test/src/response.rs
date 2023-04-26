// this should find it's way into replacing the current event stuff in levana-common eventually
use anyhow::{anyhow, Result};
use cosmos_sdk_proto::cosmos::base::abci::v1beta1::TxResponse;
use cosmwasm_std::{Attribute, Event};
use cw_multi_test::AppResponse;
use msg::prelude::*;

// for both multi-test's AppResponse and the real low-level TxResponse
// in both cases we want to be able to do things like `resp.event_first("open-position")`
// but there are a few complications (different event types, the chain adding "wasm-", etc.)
// this trait standardizes a single interface to make working with events much nicer
//
// all of the boxing of iterators can go away when existential types becomes stable
// i.e. being able to return impl Trait from a trait method
pub trait CosmosResponseExt: std::fmt::Debug {
    // this is the only method needed to satisfy
    // everything else is derived
    fn events(&self) -> Box<dyn Iterator<Item = Event>>;

    fn event_types(&self) -> Box<dyn Iterator<Item = String>> {
        Box::new(self.events().map(|evt| evt.ty))
    }

    fn filter_events_type<'a>(&self, ty: &'a str) -> Box<dyn Iterator<Item = Event> + 'a> {
        let ty = wasm_event_type(ty);
        Box::new(self.events().filter(move |e| e.ty == ty))
    }

    fn filter_events_attr<'a>(
        &self,
        ty: &'a str,
        key: &'a str,
    ) -> Box<dyn Iterator<Item = Event> + 'a> {
        let ty = wasm_event_type(ty);

        Box::new(self.events().filter(move |e| {
            if e.ty == ty {
                e.attributes.iter().any(|a| a.key == key)
            } else {
                false
            }
        }))
    }
    fn filter_events_attr_value<'a>(
        &self,
        ty: &'a str,
        key: &'a str,
    ) -> Box<dyn Iterator<Item = String> + 'a> {
        let ty = wasm_event_type(ty);

        Box::new(self.events().filter_map(move |e| {
            if e.ty == ty {
                e.attributes
                    .iter()
                    .find(|a| a.key == key)
                    .map(|a| a.value.clone())
            } else {
                None
            }
        }))
    }

    fn filter_events_map_attr_value<'a, T, F>(
        &self,
        ty: &'a str,
        key: &'a str,
        f: F,
    ) -> Box<dyn Iterator<Item = T> + 'a>
    where
        F: Clone + Fn(&str) -> Option<T> + 'a,
        T: 'static,
    {
        let ty = wasm_event_type(ty);

        Box::new(self.events().filter_map(move |e| {
            if e.ty == ty {
                e.attributes.iter().find(|a| a.key == key).and_then({
                    let f = f.clone();
                    move |a| f(a.value.as_str())
                })
            } else {
                None
            }
        }))
    }

    fn try_event_first_value(&self, ty: &str, key: &str) -> Option<String> {
        self.filter_events_attr_value(ty, key).next()
    }
    fn try_event_first(&self, ty: &str) -> Option<Event> {
        self.filter_events_type(ty).next()
    }

    fn event_first_value(&self, ty: &str, key: &str) -> Result<String> {
        self.try_event_first_value(ty, key)
            .ok_or_else(|| anyhow!("couldn't find event_first for {}.{}", ty, key))
    }
    fn event_first(&self, ty: &str) -> Result<Event> {
        self.try_event_first(ty).ok_or_else(|| {
            anyhow!("couldn't find event_first for {}", ty)
            //panic!("couldn't find event_first for {}\nin {:#?}", ty, self)
        })
    }

    fn try_event_last_value(&self, ty: &str, key: &str) -> Option<String> {
        self.filter_events_attr_value(ty, key).last()
    }
    fn try_event_last(&self, ty: &str) -> Option<Event> {
        self.filter_events_type(ty).last()
    }

    fn event_last_value(&self, ty: &str, key: &str) -> Result<String> {
        self.try_event_last_value(ty, key).ok_or_else(|| {
            anyhow!("couldn't find event_last for {}.{}", ty, key)
            //panic!("couldn't find event_last for {}.{}\nin {:#?}", ty, key, self)
        })
    }
    fn event_last(&self, ty: &str) -> Result<Event> {
        self.try_event_last(ty).ok_or_else(|| {
            anyhow!("couldn't find event_last for {}", ty)
            //panic!("couldn't find event_last for {}\nin {:#?}", ty, self)
        })
    }

    /******* Specific to responses from the Market contract **********/

    fn first_delta_neutrality_fee_amount(&self) -> Number {
        self.try_first_delta_neutrality_fee_amount().unwrap()
    }
    fn try_first_delta_neutrality_fee_amount(&self) -> Result<Number> {
        self.event_first_value("delta-neutrality-fee", "amount")?
            .try_into()
    }
}

impl CosmosResponseExt for AppResponse {
    fn events(&self) -> Box<dyn Iterator<Item = Event> + 'static> {
        Box::new(self.events.clone().into_iter())
    }
}

impl CosmosResponseExt for TxResponse {
    fn events(&self) -> Box<dyn Iterator<Item = Event> + 'static> {
        Box::new(self.events.clone().into_iter().map(|raw| {
            // Event is a non-exhaustive struct, so we must create it the "slow" way
            Event::new(raw.r#type).add_attributes(raw.attributes.into_iter().map(|attr| {
                Attribute {
                    key: String::from_utf8_lossy(&attr.key).into_owned(),
                    value: String::from_utf8_lossy(&attr.value).into_owned(),
                }
            }))
        }))
    }
}

fn wasm_event_type(ty: &str) -> String {
    format!("wasm-{}", ty)
}
