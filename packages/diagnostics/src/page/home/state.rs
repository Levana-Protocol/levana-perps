use super::init::InitUi;
use crate::prelude::*;
use futures_signals::signal::{from_future, option};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
};

pub struct HomePage {
    pub init: Rc<InitUi>,
}

impl HomePage {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            init: InitUi::new(),
        })
    }
}
