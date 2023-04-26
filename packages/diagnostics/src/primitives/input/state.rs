use crate::prelude::*;
use crate::primitives::image::Image;

pub struct Input {
    pub label: Option<String>,
    pub style: InputStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputStyle {
    Text,
    Currency,
}

impl Input {
    pub fn new(label: Option<String>, style: InputStyle) -> Rc<Self> {
        Rc::new(Self { label, style })
    }
}
