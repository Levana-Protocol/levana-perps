use dominator::DomBuilder;
use web_sys::{HtmlElement, HtmlInputElement};

use super::state::*;
use crate::prelude::*;

impl Range {
    pub fn render<S, F>(self: Rc<Self>, get_label: S, on_change: F) -> Dom
    where
        S: Fn(f64) -> String + 'static,
        F: Fn(f64) + Clone + 'static,
    {
        self.render_mixins(
            get_label,
            None::<MixinStub<HtmlElement>>,
            None::<MixinStub<HtmlInputElement>>,
            on_change,
        )
    }
    pub fn render_label_mixin<S, F, LM>(
        self: Rc<Self>,
        get_label: S,
        label_mixin: LM,
        on_change: F,
    ) -> Dom
    where
        S: Fn(f64) -> String + 'static,
        F: Fn(f64) + Clone + 'static,
        LM: FnOnce(DomBuilder<HtmlElement>) -> DomBuilder<HtmlElement> + 'static,
    {
        self.render_mixins(
            get_label,
            Some(label_mixin),
            None::<MixinStub<HtmlInputElement>>,
            on_change,
        )
    }

    pub fn render_input_mixin<S, F, IM>(
        self: Rc<Self>,
        get_label: S,
        input_mixin: IM,
        on_change: F,
    ) -> Dom
    where
        S: Fn(f64) -> String + 'static,
        F: Fn(f64) + Clone + 'static,
        IM: FnOnce(DomBuilder<HtmlInputElement>) -> DomBuilder<HtmlInputElement> + 'static,
    {
        self.render_mixins(
            get_label,
            None::<MixinStub<HtmlElement>>,
            Some(input_mixin),
            on_change,
        )
    }

    pub fn render_mixins<S, F, LM, IM>(
        self: Rc<Self>,
        get_label: S,
        label_mixin: Option<LM>,
        input_mixin: Option<IM>,
        on_change: F,
    ) -> Dom
    where
        S: Fn(f64) -> String + 'static,
        F: Fn(f64) + Clone + 'static,
        LM: FnOnce(DomBuilder<HtmlElement>) -> DomBuilder<HtmlElement> + 'static,
        IM: FnOnce(DomBuilder<HtmlInputElement>) -> DomBuilder<HtmlInputElement> + 'static,
    {
        let state = self;

        html!("div", {
            .children(&mut [
                html!("label", {
                    .attr("for", "steps-range")
                    .class(["block","mb-2","text-sm","font-medium"])
                    .apply_if(label_mixin.is_some(), |dom| dom.apply(label_mixin.unwrap_ext()))
                    .text_signal(state.value.signal_cloned().map(get_label))
                }),
                html!("input" => HtmlInputElement, {
                    .attr("id", "steps-range")
                    .attr("type", "range")
                    .attr("min", &state.opts.min.to_string())
                    .attr("max", &state.opts.max.to_string())
                    .attr("value", &state.value.get_cloned().to_string())
                    .apply_if(state.opts.step.is_some(), |dom| {
                        dom.attr("step", &state.opts.step.unwrap_ext().to_string())
                    })
                    .class(["w-full","h-2","bg-gray-200","rounded-lg","appearance-none","cursor-pointer","dark:bg-gray-700"])
                    .apply_if(input_mixin.is_some(), |dom| dom.apply(input_mixin.unwrap_ext()))
                    .with_node!(input =>  {
                        .event(clone!(on_change => move |_evt:events::Input| {
                            let value = input.value_as_number();
                            if value > f64::MIN && value < f64::MAX {
                                state.value.set(value);
                                on_change(value);
                            }
                        }))
                    })
                }),
            ])
        })
    }
}
