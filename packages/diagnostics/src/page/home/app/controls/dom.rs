use super::state::*;
use crate::page::home::app::App;
use crate::prelude::*;
use crate::primitives::button::{Button, ButtonColor};
use crate::primitives::checkbox::Checkbox;
use crate::primitives::range::{Range, RangeOpts};
use crate::runner::exec::Action;

impl Controls {
    pub fn render(self: Rc<Self>, app: Rc<App>) -> Dom {
        let state = self;

        html!("div", {
            .class(["flex", "flex-col", "items-end", "gap-4"])
            .child(html!("div", {
                .class(["flex", "items-center", "gap-4"])
                .child(Range::new(RangeOpts {
                    min: 0.1,
                    max: 100.0,
                    step: Some(0.01),
                    value: state.delay.borrow().clone(),
                }).render(
                    |secs| {
                        format!("Delay: {} secs", secs)
                    },
                    clone!(state => move |secs| {
                        *state.delay.borrow_mut() = secs;
                    })
                ))
                .child_signal(state.play_signal().map(clone!(state => move |play_state| {
                    Some(Button::new_color(ButtonColor::Primary).render_mixin(clone!(state, app => move |dom| {
                        dom
                            .text(play_state.inverse().as_str())
                            .event(clone!(state, app => move |evt:events::Click| {
                                match play_state.inverse() {
                                    PlayState::Play => app.play(),
                                    PlayState::Pause => app.pause(),
                                }
                            }))
                    })))
                })))
            }))
            .child(html!("div", {
                .class(["flex", "items-center", "gap-4"])
                .children(
                    [
                        ("open", Action::OpenPosition),
                        ("update", Action::UpdatePosition),
                        ("close", Action::ClosePosition),
                        ("price", Action::SetPrice),
                        ("time jump", Action::TimeJump),
                    ]
                    .iter()
                    .map(|(label, action)| {
                        Checkbox::new(
                            label.to_string(),
                            state.get_allow_action(*action),
                            clone!(state => move |flag| state.set_allow_action(*action,flag))
                        ).render()
                    })
                )
                .child(Checkbox::new(
                    "always crank".to_string(),
                    state.get_allow_crank(),
                    clone!(state => move |flag| state.set_allow_crank(flag))
                ).render())
            }))
        })
    }
}
