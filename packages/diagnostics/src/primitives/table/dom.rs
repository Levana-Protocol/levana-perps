use super::state::*;
use crate::prelude::*;
use dominator::svg;
use futures_signals::signal_vec::SignalVec;
use std::fmt::Display;

impl<const N: usize, S: AsRef<str>> Table<N, S> {
    pub fn render(self: Rc<Self>, children_sig: impl SignalVec<Item = [Dom; N]> + 'static) -> Dom {
        let state = self;

        let mut title_children = Vec::new();
        if let Some(title) = state.title.as_ref() {
            title_children.push(html!("h1", {
                .class(["text-xl","font-semibold","text-gray-900"])
                .text(title.as_ref())
            }));
        }
        if let Some(subtitle) = state.subtitle.as_ref() {
            title_children.push(html!("p", {
                .class(["mt-2","text-sm","text-gray-700"])
                .text(subtitle.as_ref())
            }));
        }

        let mut children = Vec::new();
        if !title_children.is_empty() {
            children.push(html!("div", {
                .class(["sm:flex","sm:items-center"])
                .children(title_children)
            }));
        }

        let header_children:Vec<Dom> = state.columns
            .iter()
            .map(|label| {
                html!("th", {
                    .attr("scope", "col")
                    .class(["sticky","top-0","z-10","border-b","border-gray-300","bg-gray-50","bg-opacity-75","py-3.5","pl-4","pr-3","text-left","text-sm","font-semibold","text-gray-900","backdrop-blur","backdrop-filter","sm:pl-6","lg:pl-8"])
                    .text(label.as_ref())
                })
            })
            .collect();

        children.push(html!("div", {
			.class(["mt-8","flex","flex-col"])
			.child(
				html!("div", {
					.class(["-my-2","-mx-4","sm:-mx-6","lg:-mx-8"])
					.child(
						html!("div", {
							.class(["inline-block","min-w-full","py-2","align-middle"])
							.child(
								html!("div", {
									.class(["shadow-sm","ring-1","ring-black","ring-opacity-5"])
									.child(
										html!("table", {
											.class(["min-w-full","border-separate"])
											.attr("style", "border-spacing: 0")
											.children([
												html!("thead", {
													.class("bg-gray-50")
													.child(
														html!("tr", {
											                .children(header_children)
                                                        })
                                                    )
                                                }),
                                                html!("tbody", {
													.class("bg-white")
													.children_signal_vec(children_sig.map(|children| {
														html!("tr", {
															.children(
                                                                children
                                                                      .into_iter()
                                                                      .map(|child| {
                                                                        html!("td", {
                                                                            .class(["whitespace-nowrap","border-b","border-gray-200","py-4","pl-4","pr-3","text-sm","font-medium","text-gray-900","sm:pl-6","lg:pl-8"])
                                                                            .child(child)
                                                                        })
                                                                      })
                                                            )
                                                        })
                                                    }))
                                                })
                                            ])
                                        })
                                    )
                                })
                            )
                        })
                    )
                })
            )
        }));

        html!("div", {
            .class(["px-4","sm:px-6","lg:px-8"])
            .children(children)
        })
    }
}
