//! Events for the farming contract
use crate::prelude::*;

/// Event emitted when a new farming contract is instantiated.
pub struct NewFarming {}

impl From<NewFarming> for Event {
    fn from(NewFarming {}: NewFarming) -> Self {
        Event::new("levana-new-farming")
    }
}
