use super::state::*;
use crate::prelude::*;

impl HomePage {
    pub fn render(self: Rc<Self>) -> Dom {
        let state = self;

        html!("div", {
            .class(["absolute", "top-0", "left-0", "w-screen", "h-screen"])
            .child(state.init.clone().render())
        })
    }
}
