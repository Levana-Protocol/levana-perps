use crate::prelude::*;

pub struct Tabs<T> {
    pub selected: Mutable<Option<Rc<TabOption<T>>>>,
    pub options: MutableVec<Rc<TabOption<T>>>,
}

impl<T: PartialEq> Tabs<T> {
    pub fn new(selected: Option<T>, options: Vec<Rc<TabOption<T>>>) -> Rc<Self> {
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
            selected: Mutable::new(selected),
            options: MutableVec::new_with_values(options),
        })
    }
}

pub struct TabOption<T> {
    pub id: T,
    pub label: String,
}

impl<T> TabOption<T> {
    pub fn new(id: T, label: String) -> Rc<Self> {
        Rc::new(Self { id, label })
    }
}
