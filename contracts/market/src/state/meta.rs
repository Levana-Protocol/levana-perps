use crate::prelude::*;

const MARKET_ID: Item<MarketId> = Item::new(namespace::MARKET_ID);

pub(crate) fn meta_init(store: &mut dyn Storage, market_id: &MarketId) -> Result<()> {
    MARKET_ID.save(store, market_id).map_err(|err| err.into())
}

impl State<'_> {
    pub(crate) fn market_id(&self, store: &dyn Storage) -> Result<&MarketId> {
        self.market_id_cache
            .get_or_try_init(|| MARKET_ID.load(store).map_err(|err| err.into()))
    }

    pub(crate) fn market_type(&self, store: &dyn Storage) -> Result<MarketType> {
        Ok(self.market_id(store)?.get_market_type())
    }
}
