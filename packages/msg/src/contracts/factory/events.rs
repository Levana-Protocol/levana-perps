//! Events emitted by the factory contract
use cosmwasm_std::{Addr, Event};
use shared::prelude::*;

/// Event when the factory instantiates a new contract.
#[derive(Debug)]
pub struct InstantiateEvent {
    /// Kind of contract instantiated
    pub kind: NewContractKind,
    /// Market ID associated with new contract
    pub market_id: MarketId,
    /// Address of the contract
    pub addr: Addr,
}

/// The type of a newly instantiate contract
#[derive(Debug, Clone, Copy)]
pub enum NewContractKind {
    /// The market
    Market,
    /// LP liquidity token proxy
    Lp,
    /// xLP liquidity token proxy
    Xlp,
    /// Position token NFT proxy
    Position,
}

impl NewContractKind {
    fn as_str(self) -> &'static str {
        match self {
            NewContractKind::Market => "market",
            NewContractKind::Lp => "lp",
            NewContractKind::Xlp => "xlp",
            NewContractKind::Position => "position",
        }
    }
}

impl FromStr for NewContractKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "market" => Ok(NewContractKind::Market),
            "lp" => Ok(NewContractKind::Lp),
            "xlp" => Ok(NewContractKind::Xlp),
            "position" => Ok(NewContractKind::Position),
            _ => Err(anyhow::anyhow!("Unknown contract kind: {s:?}")),
        }
    }
}

impl From<InstantiateEvent> for Event {
    fn from(
        InstantiateEvent {
            kind,
            market_id,
            addr,
        }: InstantiateEvent,
    ) -> Self {
        Event::new("instantiate")
            .add_attribute("kind", kind.as_str())
            .add_attribute("market-id", market_id.to_string())
            .add_attribute("addr", addr)
    }
}

impl TryFrom<Event> for InstantiateEvent {
    type Error = anyhow::Error;

    fn try_from(evt: Event) -> anyhow::Result<Self> {
        Ok(InstantiateEvent {
            kind: evt.string_attr("kind")?.parse()?,
            market_id: evt.string_attr("market-id")?.parse()?,
            addr: evt.unchecked_addr_attr("addr")?,
        })
    }
}
