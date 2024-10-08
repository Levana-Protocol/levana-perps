//! Events emitted by the position token contract
use cosmwasm_std::{Addr, Event};
use cw_utils::Expiration;
use perpswap::prelude::*;

/// New NFT was minted
#[derive(Debug)]
pub struct MintEvent {
    /// Position token ID
    pub token_id: String,
    /// Owner of the position
    pub owner: Addr,
}

impl From<MintEvent> for Event {
    fn from(src: MintEvent) -> Self {
        Event::new("mint").add_attributes(vec![
            ("token_id", src.token_id.to_string()),
            ("owner", src.owner.to_string()),
        ])
    }
}

impl TryFrom<Event> for MintEvent {
    type Error = anyhow::Error;

    fn try_from(evt: Event) -> anyhow::Result<Self> {
        Ok(MintEvent {
            token_id: evt.string_attr("token_id")?,
            owner: evt.unchecked_addr_attr("owner")?,
        })
    }
}

/// NFT was burned, aka a position was closed
#[derive(Debug)]
pub struct BurnEvent {
    /// Position that was closed
    pub token_id: String,
}

impl From<BurnEvent> for Event {
    fn from(src: BurnEvent) -> Self {
        Event::new("burn").add_attributes(vec![("token_id", src.token_id)])
    }
}

impl TryFrom<Event> for BurnEvent {
    type Error = anyhow::Error;

    fn try_from(evt: Event) -> anyhow::Result<Self> {
        Ok(BurnEvent {
            token_id: evt.string_attr("token_id")?,
        })
    }
}

// converting expiration back into an event is painful
// so these are just unidirectional for now

/// Approval was granted
#[derive(Debug)]
pub struct ApprovalEvent {
    /// Position
    pub token_id: String,
    /// Who can spend it
    pub spender: Addr,
    /// When it expires
    pub expires: Expiration,
}

impl From<ApprovalEvent> for Event {
    fn from(src: ApprovalEvent) -> Self {
        Event::new("approval").add_attributes(vec![
            ("token_id", src.token_id.to_string()),
            ("spender", src.spender.to_string()),
            ("expires", src.expires.to_string()),
        ])
    }
}

/// Approval was revoked
#[derive(Debug)]
pub struct RevokeEvent {
    /// Position ID
    pub token_id: String,
    /// Whose spend permissions were revoked
    pub spender: Addr,
}

impl From<RevokeEvent> for Event {
    fn from(src: RevokeEvent) -> Self {
        Event::new("revoke").add_attributes(vec![
            ("token_id", src.token_id.to_string()),
            ("spender", src.spender.to_string()),
        ])
    }
}

/// An operator was granted spend permissions on all positions for a wallet
#[derive(Debug)]
pub struct ApproveAllEvent {
    /// Who is the operator
    pub operator: Addr,
    /// When does the permission expire
    pub expires: Expiration,
}

impl From<ApproveAllEvent> for Event {
    fn from(src: ApproveAllEvent) -> Self {
        Event::new("approve-all").add_attributes(vec![
            ("operator", src.operator.to_string()),
            ("expires", src.expires.to_string()),
        ])
    }
}

/// Revoke all permissions for an operator
#[derive(Debug)]
pub struct RevokeAllEvent {
    /// Operator to revoke
    pub operator: Addr,
}

impl From<RevokeAllEvent> for Event {
    fn from(src: RevokeAllEvent) -> Self {
        Event::new("revoke-all").add_attributes(vec![("operator", src.operator.to_string())])
    }
}

/// NFT was transferred
#[derive(Debug)]
pub struct TransferEvent {
    /// New owner
    pub recipient: Addr,
    /// Position ID
    pub token_id: String,
}

impl From<TransferEvent> for Event {
    fn from(src: TransferEvent) -> Self {
        Event::new("transfer").add_attributes(vec![
            ("recipient", src.recipient.to_string()),
            ("token_id", src.token_id),
        ])
    }
}
