use crate::prelude::*;

#[derive(Clone, Debug)]
pub struct Progress {
    pub label: Option<String>,
    pub perc: Mutable<f32>,
}

impl Progress {
    pub fn new(label: Option<String>, perc: Option<Mutable<f32>>) -> Self {
        Self {
            label,
            perc: perc.unwrap_or_else(|| Mutable::new(0.0)),
        }
    }

    pub fn signal_string(&self) -> impl Signal<Item = String> {
        self.perc.signal().map(|perc| format!("{}%", perc * 100.0))
    }
}
