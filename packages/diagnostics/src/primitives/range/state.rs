use crate::prelude::*;

pub struct Range {
    pub value: Mutable<f64>,
    pub opts: RangeOpts,
}

impl Range {
    pub fn new(opts: RangeOpts) -> Rc<Self> {
        Rc::new(Self {
            value: Mutable::new(opts.value),
            opts,
        })
    }
}

pub struct RangeOpts {
    pub min: f64,
    pub max: f64,
    pub value: f64,
    pub step: Option<f64>,
}
