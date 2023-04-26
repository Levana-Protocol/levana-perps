use crate::prelude::*;

pub struct Table<const N: usize, S: AsRef<str>> {
    pub title: Option<S>,
    pub subtitle: Option<S>,
    pub columns: [S; N],
}

impl<const N: usize, S: AsRef<str>> Table<N, S> {
    pub fn new(title: Option<S>, subtitle: Option<S>, columns: [S; N]) -> Rc<Self> {
        Rc::new(Self {
            title,
            subtitle,
            columns,
        })
    }
}
