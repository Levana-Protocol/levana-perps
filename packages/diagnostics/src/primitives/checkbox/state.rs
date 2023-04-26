use crate::prelude::*;
use crate::primitives::image::Image;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Checkbox {
    pub label: String,
    selected: AtomicBool,
    on_change: RefCell<Box<dyn FnMut(bool)>>,
}

impl Checkbox {
    pub fn new(label: String, selected: bool, on_change: impl FnMut(bool) + 'static) -> Rc<Self> {
        Rc::new(Self {
            label,
            selected: AtomicBool::new(selected),
            on_change: RefCell::new(Box::new(on_change)),
        })
    }

    pub fn set_selected(&self, value: bool) {
        let previous = self.selected.swap(value, Ordering::SeqCst);
        if previous != value {
            self.on_change.borrow_mut()(value);
        }
    }

    pub fn get_selected(&self) -> bool {
        self.selected.load(Ordering::SeqCst)
    }
}
