use anyhow::Result;
use cosmwasm_std::{to_binary, wasm_execute, CosmosMsg, Empty, Event, Response, SubMsg, WasmMsg};
use cw2::ContractVersion;
use serde::Serialize;

#[cfg(debug_assertions)]
use crate::prelude::*;

/// Helper data type, following builder pattern, for constructing a [Response].
pub struct ResponseBuilder {
    resp: Response,
    event_type: EventType,
}

enum EventType {
    MuteEvents,
    EmitEvents {
        common_attrs: Vec<(&'static str, String)>,
    },
}

fn standard_event_attributes(
    ContractVersion { contract, version }: ContractVersion,
) -> Vec<(&'static str, String)> {
    vec![
        ("levana_protocol", "perps".to_string()),
        ("contract_version", version),
        ("contract_name", contract),
    ]
}

impl ResponseBuilder {
    /// Initialize a new builder.
    pub fn new(contract_version: ContractVersion) -> Self {
        ResponseBuilder {
            resp: Response::new(),
            event_type: EventType::EmitEvents {
                common_attrs: standard_event_attributes(contract_version),
            },
        }
    }

    /// Create a response where the event methods are no-ops.
    pub fn new_mute_events() -> Self {
        ResponseBuilder {
            resp: Response::new(),
            event_type: EventType::MuteEvents,
        }
    }

    /// Finalize the builder and generate the final response.
    pub fn into_response(self) -> Response {
        self.resp
    }

    /// Add a new [CosmosMsg] to the response.
    pub fn add_message(&mut self, msg: impl Into<CosmosMsg<Empty>>) {
        self.resp.messages.push(SubMsg::new(msg.into()));
    }

    /// Add a submessage for instantiating a new contract.
    pub fn add_instantiate_submessage<
        I: Into<u64>,
        A: Into<String>,
        L: Into<String>,
        T: Serialize,
    >(
        &mut self,
        id: I,
        admin: A,
        code_id: u64,
        label: L,
        msg: &T,
    ) -> Result<()> {
        let payload = to_binary(msg)?;

        // the common case
        // more fine-grained control via raw submessage
        let msg = WasmMsg::Instantiate {
            admin: Some(admin.into()),
            code_id,
            msg: payload,
            funds: vec![],
            label: label.into(),
        };
        self.add_raw_submessage(
            // the common case
            // more fine-grained control via raw submessage
            SubMsg::reply_on_success(msg, id.into()),
        );

        Ok(())
    }

    /// Add a new one-shot submessage execution.
    pub fn add_execute_submessage_oneshot<C: Into<String>, T: Serialize>(
        &mut self,
        contract: C,
        msg: &T,
    ) -> Result<()> {
        self.add_raw_submessage(
            // the common case
            // more fine-grained control via raw submessage
            SubMsg::new(wasm_execute(
                contract,
                msg,
                // the common case, no coins
                vec![],
            )?),
        );

        Ok(())
    }

    pub(crate) fn add_raw_submessage(&mut self, msg: SubMsg<Empty>) {
        self.resp.messages.push(msg);
    }

    /// Add an event to the response.
    pub fn add_event(&mut self, event: impl Into<Event>) {
        let event: Event = event.into();

        #[cfg(debug_assertions)]
        {
            match event.ty.as_ref() {
                "funding-payment" => debug_log!(DebugLog::FundingPaymentEvent, "{:#?}", event),
                "funding-rate-change" => {
                    debug_log!(DebugLog::FundingRateChangeEvent, "{:#?}", event)
                }
                "fee" => {
                    if let Ok(source) = event.string_attr("source") {
                        match source.as_str() {
                            "trading" => debug_log!(DebugLog::TradingFeeEvent, "{:#?}", event),
                            "borrow" => debug_log!(DebugLog::BorrowFeeEvent, "{:#?}", event),
                            "delta-neutrality" => {
                                debug_log!(DebugLog::DeltaNeutralityFeeEvent, "{:#?}", event)
                            }
                            "limit-order" => {
                                debug_log!(DebugLog::LimitOrderFeeEvent, "{:#?}", event)
                            }
                            _ => {}
                        }
                    }
                }
                "delta-neutrality-ratio" => {
                    debug_log!(DebugLog::DeltaNeutralityRatioEvent, "{:#?}", event)
                }
                _ => {}
            }
        }

        match &self.event_type {
            EventType::MuteEvents => (),
            EventType::EmitEvents { common_attrs } => self
                .resp
                .events
                .push(event.add_attributes(common_attrs.clone())),
        }
    }
}
