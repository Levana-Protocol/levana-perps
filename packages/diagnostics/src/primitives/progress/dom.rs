use super::state::*;
use crate::prelude::*;

impl Progress {
    pub fn render(self) -> Dom {
        html!("div", {
            .apply_if(self.label.is_some(), |dom| {
                dom.child(html!("div", {
                    .class(["text-center", "mb-10"])
                    .text(self.label.as_ref().unwrap_ext())
                }))
            })
            .child(html!("div", {
                .class(["w-full","bg-gray-200","rounded-full","dark:bg-gray-700"])
                .child(
                    html!("div", {
                        .class(["bg-blue-600","text-xs","font-medium","text-blue-100","text-center","p-0.5","leading-none","rounded-full"])
                        .style_signal("width", self.signal_string())
                        .text_signal(self.signal_string())
                    })
                )
            }))
        })
    }
}
