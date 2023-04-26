use super::{controls::PlayState, state::*};
use crate::{
    prelude::*,
    primitives::{
        button::{Button, ButtonColor},
        checkbox::Checkbox,
        range::{Range, RangeOpts},
    },
    runner::exec::Action,
};
use msg::{
    contracts::market::{config::Config, entry::QueryMsg},
    token::Token,
};

impl App {
    pub fn render(self: Rc<Self>) -> Dom {
        let state = self;
        html!("div", {
            .children(&mut [
                state.clone().render_header(),
                html!("div", {
                    .class(["p-8"])
                    .children(&mut [
                        state.graph.clone().render(),
                        state.event_view.clone().render(),
                    ])
                }),
            ])
        })
    }

    fn render_header(self: Rc<Self>) -> Dom {
        let state = self;
        html!("div", {
            .class(["flex", "justify-between", "items-center", "p-4", "bg-gray-100", "border-b", "border-gray-200"])
            .children(&mut [
                html!("div", {
                    .child(state.render_market_info())
                }),
                state.controls.clone().render(state.clone()),

            ])
        })
    }

    fn render_market_info(&self) -> Dom {
        html!("div", {
            .text(&format!("{} {:?}", self.market_id, self.market_type))
        })
    }
}
