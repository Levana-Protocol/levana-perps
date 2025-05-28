use cw_storage_plus::Item;
use perpswap::namespace;

/// Code ID of the vault contract id
pub(crate) const VAULT_CODE_ID: Item<u64> = Item::new(namespace::VAULT_CODE_ID);
