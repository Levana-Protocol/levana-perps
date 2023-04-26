use super::state::*;
use crate::page::home::app::App;
use crate::prelude::*;
use crate::primitives::{button::*, dropdown::*};
use web_sys::HtmlCanvasElement;

impl InitUi {
    pub fn render(self: Rc<Self>) -> Dom {
        let state = self;

        html!("div", {
            .child_signal(state.phase.signal_cloned().map(clone!(state => move |phase| {
                Some(match phase {
                    Phase::Connecting => {
                        render_init_container(html!("div", {
                            .class(["flex", "flex-col", "items-center", "text-xl", "text-purple-600"])
                            .text("Connecting to the bridge...")
                        }))
                    },
                    Phase::Error(err) => {
                        render_init_container(html!("div", {
                            .class(["flex", "flex-col", "items-center", "text-xl", "text-red-600"])
                            .text(&err)
                        }))
                    },
                    Phase::Disconnected => {
                        render_init_container(html!("div", {
                            .class(["flex", "flex-col", "items-center"])
                            .child(Button::new_color(ButtonColor::Primary).render_mixin(clone!(state => move |dom| {
                                dom
                                    .class("text-xl")
                                    .text("Connect to the bridge")
                                    .event(clone!(state => move |evt:events::Click| {
                                        state.clone().connect();
                                    }))
                            })))
                        }))
                    },
                    Phase::Connected{bridge, status} => {
                        App::new(bridge, status).render()
                    }
                })
            })))
        })
    }
}

fn render_init_container(child: Dom) -> Dom {
    html!("div", {
        .class(["absolute", "top-0", "left-0", "w-screen", "h-screen", "flex","justify-center", "items-center"])
        .child(html!("div", {
            .style("padding-bottom", "20vh")
            .child(child)
        }))
    })
}
