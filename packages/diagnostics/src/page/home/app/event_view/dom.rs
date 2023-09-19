use super::{super::state::TxEvent, state::*, CopyKind};
use crate::{
    page::home::app::{ActionHistory, ExecErrorEvent, TimeJumpEvent},
    prelude::*,
    primitives::collapsable::{Collapsable, CollapsableStyle},
};
use cosmwasm_std::Event as CosmosEvent;
use dominator::traits::MultiStr;
use msg::{
    bridge::ExecError,
    contracts::market::entry::{ExecuteMsg, ExecuteOwnerMsg},
};

impl EventView {
    pub fn render(self: Rc<Self>) -> Dom {
        let state = self;

        html!("div", {
            .child(html!("div", {
                .class(["flex", "justify-between", "items-center"])
                .child(html!("div", {
                    .class(["text-2xl", "font-bold", "mb-4", "mt-4"])
                    .text_signal(state.stats.action_history.signal_vec_cloned().len().map(|len| {
                        format!("{} most recent transactions", len)
                    }))
                }))
                .child(html!("div", {
                    .class(["flex", "gap-2"])
                    .text("copy to clipboard:")
                    .child(html!("div", {
                        .class(["flex", "gap-4"])
                        .children([
                            html!("div", {
                                .text("all")
                                .class(["text-blue-500", "cursor-pointer", "hover:underline"])
                                .event(clone!(state => move |_: events::Click| {
                                    state.copy_to_clipboard(CopyKind::All);
                                }))
                            }),
                            html!("div", {
                                .text("without events")
                                .class(["text-blue-500", "cursor-pointer", "hover:underline"])
                                .event(clone!(state => move |_: events::Click| {
                                    state.copy_to_clipboard(CopyKind::StripEvents);
                                }))
                            }),
                        ])
                    }))
                }))
            }))
            .children_signal_vec(state.stats.action_history.signal_vec_cloned().map(|action| {
                match action {
                    ActionHistory::Tx(data) => {
                        render_tx(data)
                    },
                    ActionHistory::TimeJump(data) => {
                        render_time_jump(data)
                    },
                    ActionHistory::Error(error) => {
                        render_error(error)
                    }
                }
            }))
        })
    }
}

fn render_time_jump(data: TimeJumpEvent) -> Dom {
    html!("div", {
        .class(["p-4", "rounded-t-lg","border","border-neutral-200","bg-white","dark:border-neutral-600","dark:bg-neutral-800"])
        .text(&format!("msg #{}: Time jump of {} seconds", data.msg_id, data.seconds))
    })
}

fn render_error(data: ExecErrorEvent) -> Dom {
    let label = match &data.error {
        ExecError::PerpError(error) => {
            format!("{:?} Error - {:?}", error.domain, error.id)
        }
        ExecError::Unknown(_) => "Unknown Error".to_string(),
    };
    Collapsable::new(
        label,
        CollapsableStyle::Red,
        false,
        clone!(data => move || {
            html!("div", {
                .class(["p-4", "text-white"])
                .child(match &data.error {
                    ExecError::PerpError(error) => {
                        html!("div", {
                            .child(html!("div", {
                                .text(&error.description)
                            }))
                            .apply_if(error.data.is_some(), |dom| {
                                dom.child(html!("div", {
                                    .text(&format!("data: {:?}", error.data.unwrap()))
                                }))
                            })
                        })
                    },
                    ExecError::Unknown(error) => {
                        html!("div", {
                            .text(&error)
                        })
                    }
                })
            })
        }),
    )
    .render()
}

fn render_tx(data: TxEvent) -> Dom {
    Collapsable::new(
        execute_msg_label(data.msg_id, &data.execute_msg),
        CollapsableStyle::Grey,
        false,
        move || {
            html!("div", {
                .class(["pl-4", "pr-4", "pb-4"])
                .child(html!("div", {
                    .class(["p-4"])
                    .text(&format!("took {} seconds to execute", data.msg_elapsed))
                }))
                .child(render_events(&data.events))
                .child(render_execute_msg(&data.execute_msg))
            })
        },
    )
    .render()
}

fn render_execute_msg(msg: &ExecuteMsg) -> Dom {
    Collapsable::new(
        "Execute Message".to_string(),
        CollapsableStyle::Grey,
        false,
        clone!(msg => move || {
            html!("div", {
                .class(["pl-4", "pr-4"])
                .child(html!("code", {
                    .text(&format!("{:?}", msg))
                }))
            })
        }),
    )
    .render()
}

fn render_events(events: &Vec<CosmosEvent>) -> Dom {
    Collapsable::new("Events".to_string(), CollapsableStyle::Grey, false, clone!(events => move || {
        html!("div", {
            .class(["pl-4", "pr-4"])
            .children(events.iter().map(|event| {
                Collapsable::new(event.ty.clone(), CollapsableStyle::Grey, false, clone!(event => move || {
                    html!("ul", {
                        .class(["pl-4", "pr-4"])
                        .children(event.attributes.iter().map(|attr| {
                            html!("li", {
                                .text(&format!("{}: {}", attr.key, attr.value))
                            })
                        }))
                    })
                })).render()
            }))
        })
    })).render()
}

fn execute_msg_label(msg_id: u64, execute_msg: &ExecuteMsg) -> String {
    let label = match execute_msg {
        ExecuteMsg::OpenPosition { .. } => "Open Position",
        ExecuteMsg::UpdatePositionAddCollateralImpactLeverage { .. } => {
            "Update Position (add collateral, impact leverage)"
        }
        ExecuteMsg::UpdatePositionAddCollateralImpactSize { .. } => {
            "Update Position (add collateral, impact size)"
        }
        ExecuteMsg::UpdatePositionRemoveCollateralImpactLeverage { .. } => {
            "Update Position (remove collateral, impact leverage)"
        }
        ExecuteMsg::UpdatePositionRemoveCollateralImpactSize { .. } => {
            "Update Position (remove collateral, impact size)"
        }
        ExecuteMsg::UpdatePositionLeverage { .. } => "Update Position (leverage)",
        ExecuteMsg::UpdatePositionMaxGains { .. } => "Update Position (max gains)",
        ExecuteMsg::ClosePosition { .. } => "Close Position",
        ExecuteMsg::SetManualPrice { .. } => "Set Manual Price",
        ExecuteMsg::Crank { .. } => "Crank",
        _ => "Unknown",
    };

    format!("msg #{}: {}", msg_id, label)
}
