use cosmwasm_std::{Addr, Event, Uint128};
use cw_utils::Expiration;

use super::entry::{EmbeddedLogo, Logo};

#[derive(Debug)]
pub struct TransferEvent {
    pub owner: Addr,
    pub recipient: Addr,
    pub amount: Uint128,
    pub by: Option<Addr>,
}

impl From<TransferEvent> for Event {
    fn from(src: TransferEvent) -> Self {
        let event = Event::new("transfer").add_attributes(vec![
            ("recipient", src.recipient.to_string()),
            ("amount", src.amount.to_string()),
            ("owner", src.owner.to_string()),
        ]);

        if let Some(by) = src.by {
            event.add_attribute("by", by.to_string())
        } else {
            event
        }
    }
}

#[derive(Debug)]
pub struct BurnEvent {
    pub owner: Addr,
    pub amount: Uint128,
    pub by: Option<Addr>,
}

impl From<BurnEvent> for Event {
    fn from(src: BurnEvent) -> Self {
        let event = Event::new("burn").add_attributes(vec![
            ("amount", src.amount.to_string()),
            ("owner", src.owner.to_string()),
        ]);

        if let Some(by) = src.by {
            event.add_attribute("by", by.to_string())
        } else {
            event
        }
    }
}

#[derive(Debug)]
pub struct SendEvent {
    pub owner: Addr,
    pub contract: Addr,
    pub amount: Uint128,
    pub by: Option<Addr>,
}

impl From<SendEvent> for Event {
    fn from(src: SendEvent) -> Self {
        let event = Event::new("send").add_attributes(vec![
            ("contract", src.contract.to_string()),
            ("amount", src.amount.to_string()),
            ("owner", src.owner.to_string()),
        ]);

        if let Some(by) = src.by {
            event.add_attribute("by", by.to_string())
        } else {
            event
        }
    }
}

#[derive(Debug)]
pub struct MintEvent {
    pub owner: Addr,
    pub recipient: Addr,
    pub amount: Uint128,
}

impl From<MintEvent> for Event {
    fn from(src: MintEvent) -> Self {
        Event::new("mint").add_attributes(vec![
            ("owner", src.owner.to_string()),
            ("amount", src.amount.to_string()),
            ("recipient", src.recipient.to_string()),
        ])
    }
}

#[derive(Debug)]
pub struct AllowanceChangeEvent {
    pub kind: AllowanceChangeKind,
    pub owner: Addr,
    pub spender: Addr,
    pub amount: Uint128,
    pub expires: Option<Expiration>,
}

#[derive(Debug)]
pub enum AllowanceChangeKind {
    Increase,
    Decrease,
}

impl AllowanceChangeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Increase => "increase",
            Self::Decrease => "decrease",
        }
    }
}

impl From<AllowanceChangeEvent> for Event {
    fn from(src: AllowanceChangeEvent) -> Self {
        let event = Event::new("allowance-change").add_attributes(vec![
            ("kind", src.kind.as_str().to_string()),
            ("owner", src.owner.to_string()),
            ("spender", src.spender.to_string()),
            ("amount", src.amount.to_string()),
        ]);

        match src.expires {
            Some(expires) => event.add_attribute("expires", expires.to_string()),
            None => event,
        }
    }
}

#[derive(Debug)]
pub struct MinterChangeEvent {
    pub minter: Addr,
}

impl From<MinterChangeEvent> for Event {
    fn from(src: MinterChangeEvent) -> Self {
        Event::new("minter-change").add_attribute("minter", src.minter.into_string())
    }
}

#[derive(Debug)]
pub struct MarketingChangeEvent {
    pub project: Option<String>,
    pub description: Option<String>,
    pub marketing: Option<String>,
}

impl From<MarketingChangeEvent> for Event {
    fn from(src: MarketingChangeEvent) -> Self {
        let event = Event::new("marketing-change");

        let event = match src.project {
            Some(x) => event.add_attribute("project", x),
            None => event,
        };

        let event = match src.description {
            Some(x) => event.add_attribute("description", x),
            None => event,
        };

        match src.marketing {
            Some(x) => event.add_attribute("marketing", x),
            None => event,
        }
    }
}

#[derive(Debug)]
pub struct LogoChangeEvent<'a> {
    pub logo: &'a Logo,
}

impl<'a> From<LogoChangeEvent<'a>> for Event {
    fn from(src: LogoChangeEvent) -> Self {
        let event = Event::new("logo-change");

        match src.logo {
            Logo::Url(url) => event.add_attribute("kind", "url").add_attribute("url", url),
            Logo::Embedded(embedded) => event.add_attribute("kind", "embedded").add_attribute(
                "embedded-kind",
                match embedded {
                    EmbeddedLogo::Svg(_) => "svg",
                    EmbeddedLogo::Png(_) => "png",
                },
            ),
        }
    }
}
