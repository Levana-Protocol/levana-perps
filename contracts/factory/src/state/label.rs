use msg::prelude::*;

/// Suffix attached to all contracts deployed by the factory
const LABEL_SUFFIX: Item<String> = Item::new(namespace::LABEL_SUFFIX);

pub(crate) fn set_label_suffix(store: &mut dyn Storage, label_suffix: &str) -> Result<()> {
    LABEL_SUFFIX
        .save(store, &label_suffix.to_owned())
        .map_err(|e| e.into())
}

pub(crate) fn get_label_suffix(store: &dyn Storage) -> Result<String> {
    LABEL_SUFFIX
        .may_load(store)
        .map(|x| x.unwrap_or_default())
        .map_err(|e| e.into())
}
