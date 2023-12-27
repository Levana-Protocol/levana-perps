use crate::state::*;
use shared::prelude::*;

use super::position::NEXT_LIQUIFUNDING;

/// Calculates when the protocol will become stale given the timestamp of the
/// next required liquifunding.
pub(crate) fn stale_at(config: &Config, next_liquifund: Timestamp) -> Timestamp {
    next_liquifund.plus_seconds(config.staleness_seconds.into())
}

pub(crate) struct ProtocolStaleness {
    /// Have we reached staleness of the protocol via old liquifundings? If so, contains [Option::Some], and the timestamp when that happened.
    pub(crate) stale_liquifunding: Option<Timestamp>,
}

impl State<'_> {
    /// Check the current status of staleness.
    pub(crate) fn stale_check(&self, store: &dyn Storage) -> Result<ProtocolStaleness> {
        let config = &self.config;
        let now = self.now();

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
        Ok(ProtocolStaleness { stale_liquifunding })
    }
}
