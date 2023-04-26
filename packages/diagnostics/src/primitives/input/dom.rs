use super::state::*;
use crate::prelude::*;
use dominator::DomBuilder;
use web_sys::HtmlInputElement;

impl Input {
    pub fn render(self: Rc<Self>) -> Dom {
        Self::_render_mixin(self, None::<MixinStub<HtmlInputElement>>)
    }

    pub fn render_mixin<F>(self: Rc<Self>, mixin: F) -> Dom
    where
        F: FnOnce(DomBuilder<HtmlInputElement>) -> DomBuilder<HtmlInputElement> + 'static,
    {
        Self::_render_mixin(self, Some(mixin))
    }
    pub fn _render_mixin<F>(self: Rc<Self>, mixin: Option<F>) -> Dom
    where
        F: FnOnce(DomBuilder<HtmlInputElement>) -> DomBuilder<HtmlInputElement> + 'static,
    {
        match &self.style {
            InputStyle::Text => {
                html!("input" => HtmlInputElement, {
                    .attr("type", "text")
                    .apply_if(mixin.is_some(), |dom| dom.apply(mixin.unwrap_ext()))
                })
            }
            InputStyle::Currency => {
                let mut children = match &self.label {
                    Some(label) => vec![html!("label", {
                        .attr("for", "price")
                        .class(["block","text-sm","font-medium","text-gray-700"])
                        .text(label)
                    })],
                    None => Vec::new(),
                };

                children.push(
                    html!("div", {
                        .class(["mt-2","relative","rounded-md","shadow-sm", "h-8"])
                        .children([
                            html!("div", {
                                .class(["absolute","inset-y-0","left-0","pl-3","flex","items-center","pointer-events-none"])
                                .children(&mut [
                                    html!("span", {
                                        .class(["text-gray-500","sm:text-sm"])
                                        .text("$")
                                    })
                                ])
                            }),
                            html!("input" => HtmlInputElement, {
                                .attr("type", "number")
                                .attr("name", "price")
                                .attr("id", "price")
                                .class(["focus:ring-indigo-500","focus:border-indigo-500","block","w-full","h-full","pl-7","pr-12","sm:text-sm","border-gray-300","rounded-md"])
                                .attr("placeholder", "0.00")
                                .attr("aria-describedby", "price-currency")
                                .apply_if(mixin.is_some(), |dom| dom.apply(mixin.unwrap_ext()))
                            }),
                            html!("div", {
                                .class(["absolute","inset-y-0","right-0","pr-3","flex","items-center","pointer-events-none"])
                                .children(&mut [
                                    html!("span", {
                                        .class(["text-gray-500","sm:text-sm"])
                                        .attr("id", "price-currency")
                                        .text("NEAR")
                                    })
                                ])
                            })
                        ])
                    })
                );

                html!("div", {
                    .children(children)
                })
            }
        }
    }
}
