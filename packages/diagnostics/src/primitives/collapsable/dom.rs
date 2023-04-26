use super::state::*;
use crate::prelude::*;

impl<F> Collapsable<F>
where
    F: Fn() -> Dom + 'static,
{
    pub fn render(self: Rc<Self>) -> Dom {
        let state = self;

        html!("div", {
            .class(match state.style {
                CollapsableStyle::Grey => ["rounded-t-lg","border","border-neutral-200","bg-white"],
                CollapsableStyle::Red => ["rounded-t-lg","border","border-neutral-200","bg-red-600"],
            })
            .children([
                html!("div", {
                    .class(["text-lg", "mb-0"])
                    .child(
                        html!("button", {
                            .class(match state.style {
                                CollapsableStyle::Grey => ["group","relative","flex","w-full","items-center","rounded-t-[15px]","border-0","bg-white","py-4","px-5","text-left","text-base","text-neutral-800","transition","[overflow-anchor:none]","hover:z-[2]","focus:z-[3]","focus:outline-none","[&:not([data-te-collapse-collapsed])]:bg-white","[&:not([data-te-collapse-collapsed])]:text-primary","[&:not([data-te-collapse-collapsed])]:[box-shadow:inset_0_-1px_0_rgba(229,231,235)]"],
                                CollapsableStyle::Red => ["group","relative","flex","w-full","items-center","rounded-t-[15px]","border-0","bg-red-600","py-4","px-5","text-left","text-base","text-white-800","transition","[overflow-anchor:none]","hover:z-[2]","focus:z-[3]","focus:outline-none","[&:not([data-te-collapse-collapsed])]:bg-red-600","[&:not([data-te-collapse-collapsed])]:text-white","[&:not([data-te-collapse-collapsed])]:[box-shadow:inset_0_-1px_0_rgba(229,231,235)]"]
                            })
                            .text(&state.label)
                            .child(render_chevron(state.style, state.expanded.signal()))
                            .event(clone!(state => move |_: events::Click| {
                                state.expanded.set_neq(!state.expanded.get_cloned());
                            }))
                        })
                    )
                    .child_signal(state.expanded.signal().map(clone!(state => move |expanded| {
                        if expanded {
                            Some((state.get_child)())
                        } else {
                            None
                        }
                    })))
                }),
            ])
        })
    }
}

fn render_chevron(
    style: CollapsableStyle,
    expanded_sig: impl Signal<Item = bool> + 'static,
) -> Dom {
    html!("div", {
        .class(match style {
            CollapsableStyle::Grey => ["ml-auto","h-5","w-5","shrink-0","fill-[#336dec]","transition-transform","duration-200","ease-in-out","motion-reduce:transition-none"],
            CollapsableStyle::Red => ["ml-auto","h-5","w-5","shrink-0","fill-white","transition-transform","duration-200","ease-in-out","motion-reduce:transition-none"],
        })
        .class_signal("rotate-180", expanded_sig.map(|expanded| !expanded))
        .child(
            svg!("svg", {
                .attr("xmlns", "http://www.w3.org/2000/svg")
                .attr("fill", "none")
                .attr("viewBox", "0 0 24 24")
                .attr("stroke-width", "1.5")
                .attr("stroke", "currentColor")
                .class(["h-6","w-6"])
                .child(
                    svg!("path", {
                        .attr("stroke-linecap", "round")
                        .attr("stroke-linejoin", "round")
                        .attr("d", "M19.5 8.25l-7.5 7.5-7.5-7.5")
                    })
                )
            })
        )
    })
}
