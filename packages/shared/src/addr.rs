use anyhow::{Context, Result};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Api};

/// A raw address passed in via JSON.
///
/// The purpose of this newtype wrapper is to make it clear at the type level if
/// a parameter is an address, and ensure that we go through a proper validation
/// step when using it.
#[cw_serde]
#[derive(Eq, Ord, PartialOrd)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RawAddr(String);

impl RawAddr {
    /// Validate the address into an [Addr].
    pub fn validate(&self, api: &dyn Api) -> Result<Addr> {
        api.addr_validate(&self.0)
            .with_context(|| format!("Could not parse address: {self}"))
    }

    /// Validate, but return the raw cosmwasm error
    pub fn validate_raw(&self, api: &dyn Api) -> cosmwasm_std::StdResult<Addr> {
        api.addr_validate(&self.0)
    }

    /// View the raw underlying `str`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert into the raw underlying [String]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for RawAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for RawAddr {
    fn from(s: String) -> Self {
        RawAddr(s)
    }
}

impl From<&str> for RawAddr {
    fn from(s: &str) -> Self {
        s.to_owned().into()
    }
}

impl From<Addr> for RawAddr {
    fn from(addr: Addr) -> Self {
        addr.into_string().into()
    }
}

impl From<&Addr> for RawAddr {
    fn from(addr: &Addr) -> Self {
        addr.to_string().into()
    }
}

impl From<RawAddr> for String {
    fn from(RawAddr(s): RawAddr) -> String {
        s
    }
}
