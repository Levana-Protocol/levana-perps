use crate::state::*;
use shared::prelude::*;

use super::position::NEXT_LIQUIFUNDING;

/// Calculates when the protocol will become stale given the timestamp of the
/// next required liquifunding.
pub(crate) fn stale_at(config: &Config, next_liquifund: Timestamp) -> Timestamp {
    next_liquifund.plus_seconds(config.staleness_seconds.into())
}

/// Calculates when the price will be too old given the most recent price update.
pub(crate) fn price_too_old_at(config: &Config, last_price_update: Timestamp) -> Timestamp {
    last_price_update.plus_seconds(config.price_update_too_old_seconds.into())
}

pub(crate) struct ProtocolStaleness {
    /// Have we reached staleness of the protocol via old liquifundings? If so, contains [Option::Some], and the timestamp when that happened.
    pub(crate) stale_liquifunding: Option<Timestamp>,
    /// Is the last price update too old? If so, contains [Option::Some], and the timestamp when the price became too old.
    pub(crate) old_price: Option<Timestamp>,
}

impl State<'_> {
    /// Check the current status of staleness.
    pub(crate) fn stale_check(&self, store: &dyn Storage) -> Result<ProtocolStaleness> {
        let config = &self.config;
        let now = self.now();

        let old_price = match self.spot_price(store, None).ok() {
            None => Some(self.now()),
            Some(latest_price) => {
                let price_too_old_at = price_too_old_at(config, latest_price.timestamp);
                if price_too_old_at < now {
                    Some(price_too_old_at)
                } else {
                    None
                }
            }
        };

        let stale_liquifunding = if let Some(res) = NEXT_LIQUIFUNDING
            .keys(store, None, None, cosmwasm_std::Order::Ascending)
            .next()
        {
            let (timestamp, _) = res?;
            let stale_at = stale_at(config, timestamp);
            if stale_at < now {
                Some(stale_at)
            } else {
                None
            }
        } else {
            None
        };
        Ok(ProtocolStaleness {
            old_price,
            stale_liquifunding,
        })
    }

    /// Ensure that the protocol is not currently stale and, if it is, generate an error.
    pub(crate) fn ensure_not_stale(&self, store: &dyn Storage) -> Result<()> {
        match self.stale_check(store)? {
            ProtocolStaleness {
                old_price: None,
                stale_liquifunding: None,
            } => Ok(()),
            ProtocolStaleness {
                old_price: Some(old_price),
                stale_liquifunding: None,
            } => Err(perp_anyhow!(
                ErrorId::Stale,
                ErrorDomain::Market,
                "Protocol is currently in stale state, price updates are needed (since {old_price})"
            )),
            ProtocolStaleness {
                old_price: None,
                stale_liquifunding: Some(stale_liquifunding),
            } => Err(perp_anyhow!(
                ErrorId::Stale,
                ErrorDomain::Market,
                "Protocol is currently in stale state, cranking is needed (since {stale_liquifunding})"
            )),
            ProtocolStaleness {
                old_price: Some(old_price),
                stale_liquifunding: Some(stale_liquifunding),
            } => Err(perp_anyhow!(
                ErrorId::Stale,
                ErrorDomain::Market,
                "Protocol is currently in stale state, price updates are needed (since {old_price}), cranking is needed (since {stale_liquifunding})"
            )),
        }
    }
}
