use cosmwasm_std::Env;

use crate::prelude::*;

use super::data_series::DataPoint;

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
        match INSTANTIATION_TIMESTAMP.may_load(store)? {
            Some(instantiation_timestamp) => Ok(instantiation_timestamp),
            None => {
                // Backwards compatibility for markets that were created before the instantiation timestamp was stored
                // this can be removed once it's ensured that all markets are migrated with the instantiation timestamp set
                let map: Map<Timestamp, DataPoint> =
                    Map::new(namespace::LP_BORROW_FEE_DATA_SERIES);
                let key = map
                    .keys(store, None, None, Order::Ascending)
                    .next()
                    .transpose()?
                    .context("no lp borrow fee key in instantiation time fallback")?;
                Ok(key)
            }
        }
    }
}
