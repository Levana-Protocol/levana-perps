use std::fmt::Display;

use crate::error::{ErrorDomain, PerpError};
use crate::namespace;
use crate::storage::load_external_item;
use anyhow::{anyhow, Result};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Empty, QuerierWrapper};

/// Check that an addr satisfies auth checks
#[cw_serde]
pub enum AuthCheck {
    /// The owner addr for runtime administration. not necessarily the same as migration admin
    Owner,
    /// Any specific address
    Addr(Addr),
    /// The market wind down address, used to gate the close all positions command.
    WindDown,
}

impl Display for AuthCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AuthCheck::Owner => f.write_str("protocol owner"),
            AuthCheck::Addr(a) => a.fmt(f),
            AuthCheck::WindDown => f.write_str("market wind down"),
        }
    }
}

/// Ensure that the given address passes the specified [AuthCheck].
pub fn assert_auth(
    factory_addr: &Addr,
    querier: &QuerierWrapper<Empty>,
    addr: &Addr,
    check: AuthCheck,
) -> Result<()> {
    let success = match &check {
        AuthCheck::Owner => {
            let owner_addr: Addr =
                load_external_item(querier, factory_addr, namespace::OWNER_ADDR.as_bytes())?;
            addr == owner_addr
        }
        AuthCheck::Addr(role_addr) => addr == role_addr,
        AuthCheck::WindDown => {
            let wind_down_addr: Addr =
                load_external_item(querier, factory_addr, namespace::WIND_DOWN_ADDR.as_bytes())?;
            addr == wind_down_addr
        }
    };

    if success {
        Ok(())
    } else {
        Err(anyhow!(PerpError::auth(
            ErrorDomain::Default,
            format!("failed auth, actual address: {addr}, check against: {check}")
        )))
    }
}
