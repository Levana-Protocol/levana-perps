use super::state::*;
use crate::prelude::*;
use dominator::DomBuilder;
use web_sys::HtmlElement;

impl Overlay {
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
        html!("div", {
            .class(["absolute", "w-screen", "h-screen", "top-0", "left-0", "z-50"])
            .child(html!("div", {
                .class(["absolute", "flex", "w-full", "h-full", "items-center", "justify-center", "bg-white/75"])
                .child(self.kind.render())
            }))
            .apply_if(mixin.is_some(), |dom| dom.apply(mixin.unwrap_ext()))
        })
    }
}

impl OverlayKind {
    pub fn render(&self) -> Dom {
        match self {
            Self::Loading => {
                html!("div", {
                    .text("Please wait...")
                })
            }
        }
    }
}
