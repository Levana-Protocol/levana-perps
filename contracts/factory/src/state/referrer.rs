use crate::state::*;
use anyhow::Result;
use cosmwasm_std::Addr;
use cw_storage_plus::Map;
use msg::contracts::factory::entry::ListRefereesResp;
use shared::namespace;

/// Key is a tuple of (referrer, referee)
const REFEREES_REVERSE_MAP: Map<(&Addr, &Addr), ()> = Map::new(namespace::REFEREES_REVERSE_MAP);

/// Make a lookup key for the given referee
///
/// We don't follow the normal Map pattern to simplify raw queries.
fn make_referrer_key(referee: &Addr) -> String {
    format!("ref__{}", referee.as_str())
}

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
                })
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
    REFEREES_REVERSE_MAP
        .save(store, (referrer, referee), &())
        .map_err(Into::into)
}
