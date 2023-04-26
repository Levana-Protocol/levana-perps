use crate::prelude::*;

pub struct Collapsable<F> {
    pub label: String,
    pub style: CollapsableStyle,
    pub expanded: Mutable<bool>,
    pub get_child: F,
}

impl<F> Collapsable<F> {
    pub fn new(
        label: String,
        style: CollapsableStyle,
        init_expanded: bool,
        get_child: F,
    ) -> Rc<Self> {
        Rc::new(Self {
            label,
            style,
            expanded: Mutable::new(init_expanded),
            get_child,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CollapsableStyle {
    Grey,
    Red,
}
