use crate::state::*;
use anyhow::Result;
use cosmwasm_std::Addr;
use cw_storage_plus::Map;
use msg::contracts::factory::entry::{
    make_referee_count_key, make_referrer_key, ListRefereeCountResp, ListRefereeCountStartAfter,
    ListRefereesResp, RefereeCount,
};
use shared::namespace;

/// Key is a tuple of (referrer, referee)
const REFEREES_REVERSE_MAP: Map<(&Addr, &Addr), ()> = Map::new(namespace::REFEREES_REVERSE_MAP);

/// Reverse count map for leaderboard.
const REFEREE_COUNT_REVERSE_MAP: Map<(u32, &Addr), ()> =
    Map::new(namespace::REFEREE_COUNT_REVERSE_MAP);

impl State<'_> {
    /// Look up the referrer for a given address.
    pub(crate) fn get_referrer_for(
        &self,
        store: &dyn Storage,
        referee: &Addr,
    ) -> Result<Option<Addr>> {
        store
            .get(make_referrer_key(referee).as_bytes())
            .map(|referrer_bytes| {
                RawAddr::from(String::from_utf8(referrer_bytes)?).validate(self.api)
            })
            .transpose()
    }
}

/// Get a batch of referees for a given referrer
pub(crate) fn list_referees_for(
    store: &dyn Storage,
    referrer: &Addr,
    limit: u32,
    start_after: Option<&Addr>,
) -> Result<ListRefereesResp> {
    let mut iter = REFEREES_REVERSE_MAP.prefix(referrer).range(
        store,
        start_after.map(Bound::exclusive),
        None,
        Order::Ascending,
    );
    let mut referees = vec![];
    let limit = limit.try_into()?;
    while referees.len() <= limit {
        match iter.next() {
            None => {
                return Ok(ListRefereesResp {
                    referees,
                    next_start_after: None,
                });
            }
            Some(res) => referees.push(res?.0),
        }
    }
    let has_more = iter.next().is_some();
    let next_start_after = if has_more {
        referees.last().map(|addr| addr.as_str().to_owned())
    } else {
        None
    };
    Ok(ListRefereesResp {
        referees,
        next_start_after,
    })
}

/// Set a single referrer
pub(crate) fn set_referrer_for(
    store: &mut dyn Storage,
    referee: &Addr,
    referrer: &Addr,
) -> Result<()> {
    let key = make_referrer_key(referee);
    anyhow::ensure!(
        store.get(key.as_bytes()).is_none(),
        "Cannot register a new referrer"
    );
    store.set(key.as_bytes(), referrer.as_bytes());
    REFEREES_REVERSE_MAP.save(store, (referrer, referee), &())?;

    // Update the count
    let key = make_referee_count_key(referrer);
    let count = match store.get(key.as_bytes()) {
        None => 1,
        Some(old_count) => {
            let old_count = String::from_utf8(old_count)?;
            let old_count = u32::from_str(&old_count)?;
            REFEREE_COUNT_REVERSE_MAP.remove(store, (old_count, referrer));
            old_count + 1
        }
    };

    store.set(key.as_bytes(), count.to_string().as_bytes());
    REFEREE_COUNT_REVERSE_MAP.save(store, (count, referrer), &())?;

    Ok(())
}

/// List the referee "leaderboard".
pub(crate) fn list_referee_count(
    store: &dyn Storage,
    limit: u32,
    start_after: Option<RefereeCount>,
) -> Result<ListRefereeCountResp> {
    let start_after = start_after
        .as_ref()
        .map(|RefereeCount { referrer, count }| Bound::exclusive((*count, referrer)));
    let mut iter = REFEREE_COUNT_REVERSE_MAP.range(store, None, start_after, Order::Descending);
    let mut counts = vec![];
    let limit = limit.try_into()?;
    while counts.len() <= limit {
        match iter.next() {
            None => {
                return Ok(ListRefereeCountResp {
                    counts,
                    next_start_after: None,
                });
            }
            Some(res) => {
                let (count, referrer) = res?.0;
                counts.push(RefereeCount { referrer, count })
            }
        }
    }
    let has_more = iter.next().is_some();
    let next_start_after = if has_more {
        counts.last().map(
            |RefereeCount { referrer, count }| ListRefereeCountStartAfter {
                referrer: referrer.into(),
                count: *count,
            },
        )
    } else {
        None
    };
    Ok(ListRefereeCountResp {
        counts,
        next_start_after,
    })
}
