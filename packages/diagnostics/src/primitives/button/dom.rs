use super::state::*;
use crate::prelude::*;
use dominator::DomBuilder;
use web_sys::HtmlElement;

impl Button {
    pub fn render(self: Rc<Self>, on_click: impl Fn() + 'static) -> Dom {
        Self::render_mixin(self, |dom: DomBuilder<HtmlElement>| {
            dom.event(move |_evt: events::Click| {
                on_click();
            })
        })
    }

    pub fn render_mixin<F>(self: Rc<Self>, mixin: F) -> Dom
    where
        F: FnOnce(DomBuilder<HtmlElement>) -> DomBuilder<HtmlElement> + 'static,
    {
        Self::_render_mixin(self, Some(mixin))
    }
    pub fn _render_mixin<F>(self: Rc<Self>, mixin: Option<F>) -> Dom
    where
        F: FnOnce(DomBuilder<HtmlElement>) -> DomBuilder<HtmlElement> + 'static,
    {
        match &self.style {
            ButtonStyle::Color(color) => {
                html!("button", {
                    .attr("type", "button")
                    .apply(|dom| color.mixin_color_class(dom))
                    .apply_if(mixin.is_some(), |dom| dom.apply(mixin.unwrap_ext()))
                })
            }
            ButtonStyle::Image(image) => {
                image
                    .clone()
                    .render_mixin(Some(|dom: DomBuilder<HtmlElement>| {
                        dom.style("cursor", "pointer")
                            .apply_if(mixin.is_some(), |dom| dom.apply(mixin.unwrap_ext()))
                    }))
            }
        }
    }
}

impl ButtonColor {
    pub fn mixin_color_class(&self, dom: DomBuilder<HtmlElement>) -> DomBuilder<HtmlElement> {
        match self {
            Self::Primary => dom.class([
                "bg-purple-500",
                "hover:bg-purple-600",
                "focus:ring-purple-500",
            ]),
            Self::Green => dom.class([
                "bg-emerald-500",
                "hover:bg-emerald-600",
                "focus:ring-emerald-500",
            ]),
            Self::Red => dom.class(["bg-red-500", "hover:bg-red-600", "focus:ring-red-500"]),
        }
        .class([
            "relative",
            "inline-flex",
            "items-center",
            "px-4",
            "py-2",
            "border",
            "border-transparent",
            "shadow-sm",
            "text-sm",
            "font-medium",
            "rounded-md",
            "text-white",
            "focus:outline-none",
            "focus:ring-2",
            "focus:ring-offset-2",
            "focus:ring-offset-gray-800",
        ])
    }
}
