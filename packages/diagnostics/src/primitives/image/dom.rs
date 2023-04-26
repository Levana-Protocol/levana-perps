use super::state::*;
use crate::prelude::*;
use dominator::DomBuilder;
use web_sys::HtmlElement;

impl Image {
    pub fn render(self: Rc<Self>) -> Dom {
        Self::render_mixin(self, None::<MixinStub<HtmlElement>>)
    }

    pub fn render_click(self: Rc<Self>, on_click: impl Fn() + 'static) -> Dom {
        Self::render_mixin(
            self,
            Some(|dom: DomBuilder<HtmlElement>| {
                dom.event(move |_evt: events::Click| {
                    on_click();
                })
            }),
        )
    }

    pub fn render_mixin<F>(self: Rc<Self>, mixin: Option<F>) -> Dom
    where
        F: FnOnce(DomBuilder<HtmlElement>) -> DomBuilder<HtmlElement> + 'static,
    {
        html!("img", {
            .attr("src", &self.kind.as_str())
            .apply_if(mixin.is_some(), |dom| dom.apply(mixin.unwrap_ext()))
        })
    }
}
