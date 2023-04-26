use super::state::*;
use crate::prelude::*;
use std::fmt::Display;

impl<T: Clone + PartialEq + 'static> Tabs<T> {
    pub fn render<F>(self: Rc<Self>, on_change: F) -> Dom
    where
        F: Fn(Rc<TabOption<T>>) + Clone + 'static,
    {
        let state = self;

        html!("div", {
            .class(["sm:block"])
            .child(
                html!("nav", {
                    .class(["relative","z-0","rounded-lg","shadow","flex","divide-x","divide-gray-200"])
                    .attribute("aria-label", "Tabs")

                    .children_signal_vec(
                        state.options.signal_vec_cloned()
                            .map(clone!(state, on_change => move |option| option.render(state.clone(), on_change.clone())))
                    )
                })
            )
        })
    }
}

impl<T: Clone + PartialEq + 'static> TabOption<T> {
    pub fn render<F>(self: Rc<Self>, tabs: Rc<Tabs<T>>, on_change: F) -> Dom
    where
        F: Fn(Rc<TabOption<T>>) + Clone + 'static,
    {
        let state = self;
        let selected_sig = || {
            tabs.selected.signal_ref(clone!(state => move |x| {
                match x {
                    None => false,
                    Some(x) => x.id == state.id
                }
            }))
        };
        let not_selected_sig = || selected_sig().map(|x| !x);

        html!("div", {
            .class(["cursor-pointer", "rounded-l-lg","group","relative","min-w-0","flex-1","overflow-hidden","bg-white","py-4","px-4","text-sm","font-medium","text-center","hover:bg-gray-50","focus:z-10"])
            .class_signal("text-gray-900", selected_sig())
            .class_signal(["text-gray-500", "hover:text-gray-700"], not_selected_sig())
            .attribute("aria-current", "page")
            .children([
                html!("span", {
                    .text(&state.label)
                }),
                html!("span", {
                    .attribute("aria-hidden", "true")
                    .class_signal("bg-indigo-500", selected_sig())
                    .class_signal("bg-transparent", not_selected_sig())
                    .class(["absolute","inset-x-0","bottom-0","h-0.5"])
                }),
            ])
            .event(clone!(tabs => move |_evt:events::Click| {
                on_change(state.clone());
                tabs.selected.set(Some(state.clone()));
            }))
        })
    }
}
