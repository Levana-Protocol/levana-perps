use std::collections::VecDeque;

use super::state::Graph;
use crate::{page::home::app::stats::NumberExt, prelude::*};
use msg::token::Token;

impl Graph {
    pub fn render(self: Rc<Self>) -> Dom {
        let state = self;
        html!("div", {
            .class(["flex"])
            .style("min-height", "calc(50vw * (9 / 16))")
            .children([
                state.render_text(),
                state.clone().render_canvas(),
            ])
        })
    }

    fn render_canvas(self: Rc<Self>) -> Dom {
        let state = self;
        html!("canvas" => web_sys::HtmlCanvasElement, {
            .class(["w-1/2"])
            .style("height", "calc(50vw * (9 / 16))")
            .class_signal("hidden", state.controls.show_graph.signal().map(|value| !value))
            .after_inserted(clone!(state => move |canvas| {
                state.clone().start_render_loop(&canvas).unwrap();
            }))
            .with_node!(canvas => {
                .event(clone!(state => move |evt:events::MouseDown| {
                }))
                .event(clone!(state => move |evt:events::Wheel| {
                }))
                .global_event(clone!(state => move |evt:events::KeyDown| {
                }))
                .global_event(clone!(state => move |evt:events::KeyUp| {
                }))
                .global_event(clone!(state => move |evt:events::MouseMove| {
                }))
                .global_event(clone!(state => move |evt:events::MouseUp| {
                }))
            })
        })
    }

    fn render_text(&self) -> Dom {
        html!("div", {
            .class(["w-1/2", "h-full"])
            .children([
                html!("div", {
                    .text_signal(self.stats.position_ids.signal_ref(|values| format!("Open Positions: {}", values.len())))
                }),
                html!("div", {
                    .text_signal(self.stats.deposit_collateral.signal_ref(|value| extract_value("Deposit Collateral", *value)))
                }),
                html!("div", {
                    .text_signal(self.stats.trade_volume.signal_ref(|value| extract_value("Trade Volume", *value)))
                }),
                html!("div", {
                    .text_signal(self.stats.realized_pnl.signal_ref(|value| extract_value("Realized PnL", *value)))
                }),
                html!("div", {
                    .text_signal(self.stats.price.signal_ref(|value| match value {
                        Some(value) => extract_value("Price", *value),
                        None => "Price: N/A".to_string(),
                    }))
                }),
            ])
            .child_signal(self.stats.market_status.signal_ref(|market_status| {
                market_status.as_ref().map(|market_status| {
                    html!("div", {
                        .children([
                            html!("div", { .text(&extract_value("long interest (notional)", market_status.long_notional)) }),
                            html!("div", { .text(&extract_value("short interest (notional)", market_status.short_notional)) }),
                            html!("div", { .text(&extract_value("long interest (usd)", market_status.long_usd)) }),
                            html!("div", { .text(&extract_value("short interest (usd)", market_status.short_usd)) }),
                            html!("div", { .text(&extract_value("delta neutrality fee", market_status.instant_delta_neutrality_fee_value)) }),
                            html!("div", { .text(&extract_value("wallet fee", market_status.fees.wallets)) }),
                            html!("div", { .text(&extract_value("protocol fee", market_status.fees.protocol)) }),
                            html!("div", { .text(&extract_value("crank fee", market_status.fees.crank)) }),
                        ])
                    })
                })
            }))
        })
    }
}

fn extract_value(label: &str, value: impl NumberExt) -> String {
    format!("{}: {}", label, value.into_number_ext())
}
