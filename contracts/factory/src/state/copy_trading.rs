use cosmwasm_std::{StdResult, Storage, Uint64};
use cw_storage_plus::{IntKey, Item, Key, KeyDeserialize, Map, PrimaryKey};
use perpswap::contracts::factory::entry::{CopyTradingAddr, LeaderAddr};
use perpswap::namespace;
use perpswap::time::Timestamp;

/// Code ID of the copy trading contract
pub(crate) const COPY_TRADING_CODE_ID: Item<u64> = Item::new(namespace::COPY_TRADING_CODE_ID);

/// Contains the mapping of wallet and the copy trading contract address
pub(crate) const COPY_TRADING_ADDRS: Map<(LeaderAddr, CopyTradingAddr), CopyTradingId> =
    Map::new(namespace::COPY_TRADING_ADDRS);

/// Reverse of the COPY_TRADING_ADDRS store
pub(crate) const COPY_TRADING_ADDRS_REVERSE: Map<CopyTradingId, (LeaderAddr, CopyTradingAddr)> =
    Map::new(namespace::COPY_TRADING_ADDRS_REVERSE);

/// Total copy trading contracts inserted so far
pub(crate) const COPY_TRADING_TOTAL_CONTRACTS: Item<u64> =
    Item::new(namespace::COPY_TRADING_TOTAL_CONTRACTS);

/// Timestamp when new copy trading contract was added last
pub(crate) const COPY_TRADING_LAST_ADDED: Item<Timestamp> =
    Item::new(namespace::COPY_TRADING_LAST_ADDED);

/// Queue position number
#[derive(
    Copy, PartialOrd, Ord, Eq, Clone, PartialEq, serde::Serialize, serde::Deserialize, Debug,
)]
#[serde(rename_all = "snake_case")]
pub struct CopyTradingId(Uint64);

impl CopyTradingId {
    /// Construct a new value from a [u64].
    pub fn new(x: u64) -> Self {
        CopyTradingId(x.into())
    }

    /// The underlying `u64` representation.
    pub fn u64(self) -> u64 {
        self.0.u64()
    }

    /// Generate the next position ID
    ///
    /// Panics on overflow
    pub fn next(self) -> Self {
        CopyTradingId((self.u64() + 1).into())
    }
}

impl<'a> PrimaryKey<'a> for CopyTradingId {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Val64(self.0.u64().to_cw_bytes())]
    }
}

impl KeyDeserialize for CopyTradingId {
    type Output = CopyTradingId;

    const KEY_ELEMS: u16 = 1;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        u64::from_vec(value).map(|x| CopyTradingId(Uint64::new(x)))
    }
}

impl KeyDeserialize for &CopyTradingId {
    type Output = CopyTradingId;

    const KEY_ELEMS: u16 = 1;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        let trading_id = <CopyTradingId as KeyDeserialize>::from_vec(value)?;
        Ok(trading_id)
    }
}

pub(crate) fn store_new_copy_trading_contract(
    storage: &mut dyn Storage,
    leader: LeaderAddr,
    contract: CopyTradingAddr,
) -> anyhow::Result<()> {
    let total_contracts = COPY_TRADING_TOTAL_CONTRACTS.may_load(storage)?;
    let contract_index = match total_contracts {
        Some(total) => CopyTradingId::new(total),
        None => CopyTradingId::new(0),
    };
    COPY_TRADING_ADDRS.save(storage, (leader.clone(), contract.clone()), &contract_index)?;
    COPY_TRADING_ADDRS_REVERSE.save(storage, contract_index, &(leader, contract))?;
    let new_total = contract_index.next();
    COPY_TRADING_TOTAL_CONTRACTS.save(storage, &new_total.u64())?;
    Ok(())
}
