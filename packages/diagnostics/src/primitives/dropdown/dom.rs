use super::state::*;
use crate::prelude::*;
use dominator::svg;
use futures_signals::signal::ReadOnlyMutable;
use std::fmt::Display;
use wasm_bindgen::JsCast;

impl<T: Clone + PartialEq + 'static> Dropdown<T> {
    pub fn render<F>(self: Rc<Self>, on_change: F) -> Dom
    where
        F: Fn(Rc<DropdownOption<T>>) + Clone + 'static,
    {
        let state = self;

        html!("div", {
            .class(["relative","inline-block","text-left"])
            .children([
                html!("div", {
                    .child(html!("button", {
                        .attr("type", "button")
                        .class(["inline-flex","justify-center","w-full","rounded-md","border","border-gray-300","shadow-sm","px-4","py-2","bg-white","text-sm","font-medium","text-gray-700","hover:bg-gray-50","focus:outline-none"])
                        .attr("id", "menu-button")
                        .attr("aria-expanded", "true")
                        .attr("aria-haspopup", "true")
                        .children([
                            html!("div", {
                                .class("pointer-events-none")
                                .text_signal(state.selected.signal_cloned().map(clone!(state => move |selected| {
                                    match selected {
                                        Some(selected) => selected.label.clone(),
                                        None => state.label_empty.clone()
                                    }
                                })))
                            }),
                            svg!("svg", {
                                .class("pointer-events-none")
                                .class(["-mr-1","ml-2","h-5","w-5"])
                                .attr("xmlns", "http://www.w3.org/2000/svg")
                                .attr("viewBox", "0 0 20 20")
                                .attr("fill", "currentColor")
                                .attr("aria-hidden", "true")
                                .children(&mut [
                                    svg!("path", {
                                        .attr("fill-rule", "evenodd")
                                        .attr("d", "M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z")
                                        .attr("clip-rule", "evenodd")
                                    }),
                                ])
                            }),
                        ])
                        //.event(clone!(state => move |evt:events::Click| {
                            //state.open.replace_with(|x| !*x);
                        //}))
                        .with_node!(elem => {
                            .global_event(clone!(state => move |evt:events::Click| {
                                let is_target = match evt.target() {
                                    None => false,
                                    Some(target) => {
                                        let target: web_sys::HtmlElement = target.unchecked_into();
                                        target == elem
                                    }
                                };
                                if is_target {
                                    state.open.replace_with(|x| !*x);
                                } else {
                                    if state.open.get_cloned() {
                                        state.open.set(false);
                                    }
                                }
                            }))
                        })
                    }))
                }),
                html!("div", {
                    .class_signal("hidden", state.open.signal().map(|x| !x))
                    .class(["z-50", "transition-transform", "origin-middle","absolute","left-0","mt-2","w-56","rounded-md","shadow-lg","bg-white","ring-1","ring-black","ring-opacity-5","focus:outline-none"])
                    .attr("role", "menu")
                    .attr("aria-orientation", "vertical")
                    .attr("aria-labelledby", "menu-button")
                    .attr("tabindex", "-1")
                    .child(html!("div", {
                        .class("py-1")
                        .attr("role", "none")
                        .children_signal_vec(
                            state.options.signal_vec_cloned()
                                .map(clone!(state, on_change => move |option| option.render(state.clone(), on_change.clone())))
                        )
                    }))

                }),
            ])
            .event(clone!(state => move |evt:events::KeyUp| {
                if state.open.get() && evt.key() == "Escape" {
                    state.open.set(false);
                }
            }))
        })
    }
}

impl<T: Clone + PartialEq + 'static> DropdownOption<T> {
    pub fn render<F>(self: Rc<Self>, dropdown: Rc<Dropdown<T>>, on_change: F) -> Dom
    where
        F: Fn(Rc<DropdownOption<T>>) + Clone + 'static,
    {
        let state = self;
        let selected_sig = || {
            dropdown.selected.signal_ref(clone!(state => move |x| {
                match x {
                    None => false,
                    Some(x) => x.id == state.id
                }
            }))
        };
        let not_selected_sig = || selected_sig().map(|x| !x);

        html!("div", {
            .class_signal(["bg-white-100", "text-gray-900"], selected_sig())
            .class_signal("text-gray-700", not_selected_sig())
            .class(["text-gray-700","block","px-4","py-2","text-sm", "cursor-pointer", "hover:bg-indigo-100"])
            .attr("role", "menuitem")
            .attr("tabindex", "-1")
            .text(&state.label)
            .event(clone!(dropdown => move |_evt:events::Click| {
                on_change(state.clone());
                dropdown.selected.set(Some(state.clone()));
            }))

        })
    }
}
