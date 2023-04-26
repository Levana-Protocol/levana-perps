use super::state::*;
use crate::prelude::*;

impl NotFoundPage {
    pub fn render(self: Rc<Self>) -> Dom {
        let state = self;

        html!("div", {
            .class(["flex", "w-full"])
        })
    }
}
