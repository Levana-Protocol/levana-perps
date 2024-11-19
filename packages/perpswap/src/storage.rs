//! Helpers for dealing with CosmWasm storage.

pub use crate::prelude::*;
use cosmwasm_std::{from_json, Binary, Empty, QuerierWrapper};
use cw_storage_plus::{KeyDeserialize, Prefixer, PrimaryKey};

/// A multilevel [Map] where the suffix of the key monotonically increases.
///
/// This represents a common pattern where we want to store a data series by
/// some key, such as a series of position events per position. The [u64] is
/// guaranteed to monotonically increase over time per `K` value.
pub type MonotonicMultilevelMap<'a, K, T> = Map<(K, u64), T>;

/// Push a new value to a [MonotonicMultilevelMap].
pub fn push_to_monotonic_multilevel_map<'a, K, T>(
    store: &mut dyn Storage,
    m: MonotonicMultilevelMap<'a, K, T>,
    k: K,
    t: &T,
) -> Result<u64>
where
    K: PrimaryKey<'a> + Prefixer<'a> + KeyDeserialize,
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let next_id = m
        .prefix(k.clone())
        .keys(store, None, None, cosmwasm_std::Order::Descending)
        .next()
        .transpose()?
        .map_or(0, |x| x + 1);
    m.save(store, (k, next_id), t)?;
    Ok(next_id)
}

/// Helper to paginate over [MonotonicMultilevelMap]
pub fn collect_monotonic_multilevel_map<'a, K, T>(
    store: &dyn Storage,
    m: MonotonicMultilevelMap<'a, K, T>,
    k: K,
    after_id: Option<u64>,
    limit: Option<u32>,
    order: Order,
) -> Result<Vec<(u64, T)>>
where
    K: PrimaryKey<'a> + Prefixer<'a> + KeyDeserialize,
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let iter = m
        .prefix(k)
        .range(store, after_id.map(Bound::exclusive), None, order)
        .map(|res| res.map_err(|err| err.into()));

    match limit {
        Some(limit) => iter.take(limit.try_into()?).collect(),
        None => iter.collect(),
    }
}

/// A [Map] where the key monotonically increases.
///
/// This represents a common pattern where we want to store data with unique keys
/// The [u64] key is guaranteed to monotonically increase over time per pushed value.
pub type MonotonicMap<'a, T> = Map<u64, T>;

/// Push a new value to a [MonotonicMap].
pub fn push_to_monotonic_map<T>(store: &mut dyn Storage, m: MonotonicMap<T>, t: &T) -> Result<u64>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let next_id = m
        .keys(store, None, None, cosmwasm_std::Order::Descending)
        .next()
        .transpose()?
        .map_or(0, |x| x + 1);
    m.save(store, next_id, t)?;
    Ok(next_id)
}

/// Helper to paginate over [MonotonicMap]
pub fn collect_monotonic_map<T>(
    store: &dyn Storage,
    m: MonotonicMap<T>,
    after_id: Option<u64>,
    limit: Option<u32>,
    order: Order,
) -> Result<Vec<(u64, T)>>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let iter = m
        .range(store, after_id.map(Bound::exclusive), None, order)
        .map(|res| res.map_err(|err| err.into()));

    match limit {
        Some(limit) => iter.take(limit.try_into()?).collect(),
        None => iter.collect(),
    }
}

/// Load an [cw_storage_plus::Item] stored in an external contract
pub fn load_external_item<T: serde::de::DeserializeOwned>(
    querier: &QuerierWrapper<Empty>,
    contract_addr: impl Into<String>,
    key: impl Into<Binary>,
) -> anyhow::Result<T> {
    // only deserialize for extra context if in debug mode
    // because we must pass the key in as an owned value
    // and so we have to extract the name in the happy path too
    let key: Binary = key.into();
    let debug_key_name = if cfg!(debug_assertions) {
        from_json::<String>(&key).ok()
    } else {
        None
    };

    external_helper(querier, contract_addr, key, || {
        anyhow!(PerpError::new(
            ErrorId::Any,
            ErrorDomain::Default,
            format!(
                "unable to load external item {}",
                debug_key_name.unwrap_or_default()
            )
        ))
    })
}

/// Load a value from a [cw_storage_plus::Map] stored in an external contract
pub fn load_external_map<'a, T: serde::de::DeserializeOwned>(
    querier: &QuerierWrapper<Empty>,
    contract_addr: impl Into<String>,
    namespace: &str,
    key: &impl PrimaryKey<'a>,
) -> anyhow::Result<T> {
    external_helper(querier, contract_addr, map_key(namespace, key), || {
        anyhow!(PerpError::new(
            ErrorId::Any,
            ErrorDomain::Default,
            format!("unable to load external map {}", namespace)
        ))
    })
}

/// Check if a [cw_storage_plus::Map] in an external contract has a specific key
pub fn external_map_has<'a>(
    querier: &QuerierWrapper<Empty>,
    contract_addr: impl Into<String>,
    namespace: &str,
    key: &impl PrimaryKey<'a>,
) -> anyhow::Result<bool> {
    querier
        .query_wasm_raw(contract_addr, map_key(namespace, key))
        .map(|x| x.is_some())
        .map_err(|e| e.into())
}

fn external_helper<T: serde::de::DeserializeOwned>(
    querier: &QuerierWrapper<Empty>,
    contract_addr: impl Into<String>,
    key: impl Into<Binary>,
    mk_error_message: impl FnOnce() -> anyhow::Error,
) -> anyhow::Result<T> {
    let res = querier
        .query_wasm_raw(contract_addr, key)?
        .ok_or_else(mk_error_message)?;
    serde_json_wasm::from_slice(&res).map_err(|err| err.into())
}
/// Generate a storage key for a value in a [cw_storage_plus::Map].
pub fn map_key<'a, K: PrimaryKey<'a>>(namespace: &str, key: &K) -> Vec<u8> {
    // Taken from https://github.com/CosmWasm/cw-storage-plus/blob/69300779519d8ba956fb53725e44e2b59c317b1c/src/helpers.rs#L57
    // If only that was exposed...

    let mut size = namespace.len();
    let key = key.key();
    assert!(!key.is_empty());

    for x in &key {
        size += x.as_ref().len() + 2;
    }

    let mut out = Vec::<u8>::with_capacity(size);

    for prefix in std::iter::once(namespace.as_bytes())
        .chain(key.iter().take(key.len() - 1).map(|key| key.as_ref()))
    {
        out.extend_from_slice(&encode_length(prefix));
        out.extend_from_slice(prefix);
    }

    if let Some(last) = key.last() {
        out.extend_from_slice(last.as_ref());
    }

    out
}

fn encode_length(bytes: &[u8]) -> [u8; 2] {
    if let Ok(len) = u16::try_from(bytes.len()) {
        len.to_be_bytes()
    } else {
        panic!("only supports namespaces up to length 0xFFFF")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::mock_dependencies;
    use cw_storage_plus::Map;

    #[test]
    fn simple_keys_work() {
        let mut deps = mock_dependencies();
        let m = Map::<&str, String>::new("foobarbazbin");
        m.save(&mut deps.storage, "somekey", &"somevalue".to_owned())
            .unwrap();
        let key = map_key("foobarbazbin", &"somekey");
        assert_eq!(deps.as_ref().storage.get(&key).unwrap(), b"\"somevalue\"");
    }

    #[test]
    fn complex_keys_work() {
        const NAMESPACE: &str = "👋";
        let mut deps = mock_dependencies();
        let m = Map::<ComplexKey, String>::new(NAMESPACE);
        let key = (
            ("level1".to_owned(), "level™️💪".to_owned()),
            "level-".to_owned(),
        );
        let storage_key = map_key(NAMESPACE, &key);
        m.save(&mut deps.storage, key, &"somevalue".to_owned())
            .unwrap();
        assert_eq!(
            deps.as_ref().storage.get(&storage_key).unwrap(),
            b"\"somevalue\""
        );
    }

    type ComplexKey = ((String, String), String);
}
