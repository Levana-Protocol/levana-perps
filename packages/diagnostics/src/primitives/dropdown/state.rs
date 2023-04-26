use crate::prelude::*;

pub struct Dropdown<T> {
    pub label_empty: String,
    pub selected: Mutable<Option<Rc<DropdownOption<T>>>>,
    pub open: Mutable<bool>,
    pub options: MutableVec<Rc<DropdownOption<T>>>,
}

impl<T: PartialEq> Dropdown<T> {
    pub fn new(
        label_empty: String,
        selected: Option<T>,
        options: Vec<Rc<DropdownOption<T>>>,
    ) -> Rc<Self> {
        let selected = match selected {
            None => None,
            Some(selected) => options.iter().find_map(|x| {
                if x.id == selected {
                    Some(x.clone())
                } else {
                    None
                }
            }),
        };

        Rc::new(Self {
            label_empty,
            selected: Mutable::new(selected),
            open: Mutable::new(false),
            options: MutableVec::new_with_values(options),
        })
    }
}

pub struct DropdownOption<T> {
    pub id: T,
    pub label: String,
}

impl<T> DropdownOption<T> {
    pub fn new(id: T, label: String) -> Rc<Self> {
        Rc::new(Self { id, label })
    }
}
