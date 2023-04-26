use crate::prelude::*;
use crate::primitives::image::Image;

pub struct Button {
    pub style: ButtonStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ButtonStyle {
    Color(ButtonColor),
    Image(Rc<Image>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonColor {
    Primary,
    Green,
    Red,
}

impl Button {
    pub fn new_color(color: ButtonColor) -> Rc<Self> {
        Rc::new(Self {
            style: ButtonStyle::Color(color),
        })
    }
    pub fn new_image(image: Rc<Image>) -> Rc<Self> {
        Rc::new(Self {
            style: ButtonStyle::Image(image),
        })
    }
}
