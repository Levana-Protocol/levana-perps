use cosmwasm_std::Env;

use crate::prelude::*;

const MARKET_ID: Item<MarketId> = Item::new(namespace::MARKET_ID);
const INSTANTIATION_TIMESTAMP: Item<Timestamp> = Item::new(namespace::INSTANTIATION_TIMESTAMP);

pub(crate) fn meta_init(store: &mut dyn Storage, env: &Env, market_id: &MarketId) -> Result<()> {
    MARKET_ID.save(store, market_id)?;
    INSTANTIATION_TIMESTAMP.save(store, &env.block.time.into())?;

    Ok(())
}

impl State<'_> {
    pub(crate) fn market_id(&self, store: &dyn Storage) -> Result<&MarketId> {
        self.market_id_cache
            .get_or_try_init(|| MARKET_ID.load(store).map_err(|err| err.into()))
    }

    pub(crate) fn market_type(&self, store: &dyn Storage) -> Result<MarketType> {
        Ok(self.market_id(store)?.get_market_type())
    }

    pub(crate) fn instantiation_time(&self, store: &dyn Storage) -> Result<Timestamp> {
        INSTANTIATION_TIMESTAMP
            .load(store)
            .map_err(|err| err.into())
    }
}
