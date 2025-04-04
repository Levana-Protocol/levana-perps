use std::{collections::HashSet, ops::AddAssign, sync::Arc};

use anyhow::Result;
use chrono::{DateTime, Utc};
use cosmos::Address;
use dashmap::DashMap;
use itertools::Itertools;
use perpswap::contracts::market::{
    config::Config,
    spot_price::{PythConfig, PythPriceServiceNetwork, SpotPriceConfig, SpotPriceFeedData},
};
use pyth_sdk_cw::PriceIdentifier;

use crate::app::App;

impl App {
    pub(crate) async fn pyth_prices_closed(
        &self,
        address: Address,
        config: &Config,
    ) -> Result<bool> {
        let lock = self
            .pyth_market_hours
            .cache
            .entry(address)
            .or_default()
            .clone();
        let mut guard = lock.lock().await;
        let now = Utc::now();
        if let Some(is_open) = guard.as_ref().filter(|x| x.valid_until > now) {
            return Ok(!is_open.is_open);
        }

        let ids = self.pyth_market_hours.get_ids(address, config);

        let mut is_open = IsOpen {
            is_open: true,
            valid_until: now + chrono::Duration::seconds(600),
        };
        for id in &*ids.ids {
            let url = format!("https://querier-mainnet.levana.finance/v1/pyth/market-hours/{id}");
            is_open += self
                .client
                .get(&url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
        }

        let res = Ok(!is_open.is_open);
        *guard = Some(is_open);
        res
    }
}

#[derive(Default)]
pub(crate) struct PythMarketHours {
    cache: DashMap<Address, Arc<tokio::sync::Mutex<Option<IsOpen>>>>,
    ids: DashMap<Address, IdsCache>,
}

impl PythMarketHours {
    fn get_ids(&self, address: Address, config: &Config) -> IdsCache {
        let cached = self.ids.entry(address).or_default().clone();
        let ids = get_ids_from_config(config);
        if ids != cached {
            self.ids.insert(address, ids.clone());
            return ids;
        }
        cached
    }
}

fn get_ids_from_config(config: &Config) -> IdsCache {
    match &config.spot_price {
        SpotPriceConfig::Manual { .. } => IdsCache::default(),
        SpotPriceConfig::Oracle {
            pyth,
            stride: _,
            feeds,
            feeds_usd,
            volatile_diff_seconds: _,
        } => {
            if is_pyth_stable(pyth.as_ref()) {
                let ids = feeds
                    .iter()
                    .chain(feeds_usd)
                    .flat_map(|feed| match feed.data {
                        SpotPriceFeedData::Constant { .. } => None,
                        SpotPriceFeedData::Pyth {
                            id,
                            age_tolerance_seconds: _,
                        } => Some(id),
                        SpotPriceFeedData::Stride { .. } => None,
                        SpotPriceFeedData::Sei { .. } => None,
                        SpotPriceFeedData::Rujira { .. } => None,
                        SpotPriceFeedData::Simple { .. } => None,
                    })
                    // Get rid of duplicates
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .sorted()
                    .collect_vec()
                    .into_boxed_slice()
                    .into();
                IdsCache { ids }
            } else {
                IdsCache::default()
            }
        }
    }
}

fn is_pyth_stable(pyth: Option<&PythConfig>) -> bool {
    match pyth {
        Some(pyth) => match pyth.network {
            PythPriceServiceNetwork::Stable => true,
            PythPriceServiceNetwork::Edge => false,
        },
        None => false,
    }
}

#[derive(serde::Deserialize, Debug)]
struct IsOpen {
    is_open: bool,
    valid_until: DateTime<Utc>,
}

impl AddAssign for IsOpen {
    fn add_assign(&mut self, rhs: Self) {
        self.is_open = self.is_open && rhs.is_open;
        self.valid_until = self.valid_until.min(rhs.valid_until);
    }
}

#[derive(Clone, PartialEq, Eq)]
struct IdsCache {
    ids: Arc<[PriceIdentifier]>,
}

impl Default for IdsCache {
    fn default() -> Self {
        Self { ids: Arc::new([]) }
    }
}
