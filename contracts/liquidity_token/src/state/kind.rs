use cw_storage_plus::Item;
use perpswap::contracts::liquidity_token::LiquidityTokenKind;
use perpswap::prelude::*;

const TOKEN_KIND: Item<LiquidityTokenKind> = Item::new(namespace::TOKEN_KIND);

pub(crate) fn get_kind(store: &dyn Storage) -> Result<LiquidityTokenKind> {
    TOKEN_KIND.load(store).map_err(|err| err.into())
}

pub(crate) fn kind_init(store: &mut dyn Storage, kind: LiquidityTokenKind) -> Result<()> {
    TOKEN_KIND.save(store, &kind)?;
    Ok(())
}
