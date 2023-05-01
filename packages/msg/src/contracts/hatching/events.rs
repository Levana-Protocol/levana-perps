use super::HatchDetails;
use shared::prelude::*;

pub struct HatchStartEvent {
    pub id: u64,
    pub details: HatchDetails,
}
impl PerpEvent for HatchStartEvent {}
impl From<HatchStartEvent> for cosmwasm_std::Event {
    fn from(src: HatchStartEvent) -> Self {
        mixin_hatch_event(cosmwasm_std::Event::new("hatch-start"), src.id, src.details)
    }
}

pub struct HatchRetryEvent {
    pub id: u64,
    pub details: HatchDetails,
}
impl PerpEvent for HatchRetryEvent {}
impl From<HatchRetryEvent> for cosmwasm_std::Event {
    fn from(src: HatchRetryEvent) -> Self {
        mixin_hatch_event(cosmwasm_std::Event::new("hatch-retry"), src.id, src.details)
    }
}

pub struct HatchCompleteEvent {
    pub id: u64,
    pub details: HatchDetails,
}
impl PerpEvent for HatchCompleteEvent {}
impl From<HatchCompleteEvent> for cosmwasm_std::Event {
    fn from(src: HatchCompleteEvent) -> Self {
        mixin_hatch_event(
            cosmwasm_std::Event::new("hatch-complete"),
            src.id,
            src.details,
        )
    }
}

fn mixin_hatch_event(
    mut event: cosmwasm_std::Event,
    hatch_id: u64,
    details: HatchDetails,
) -> cosmwasm_std::Event {
    event = event
        .add_attribute("hatch-id", hatch_id.to_string())
        .add_attribute("eggs-len", details.eggs.len().to_string())
        .add_attribute("dusts-len", details.dusts.len().to_string())
        .add_attribute("original-owner", details.original_owner.to_string())
        .add_attribute("nft-mint-owner", details.nft_mint_owner.to_string());

    for (i, egg) in details.eggs.into_iter().enumerate() {
        event = event
            .add_attribute(format!("egg-token-id-{i}"), egg.token_id.to_string())
            .add_attribute(
                format!("egg-spirit-level-{i}"),
                egg.spirit_level.to_string(),
            );
    }
    for (i, dust) in details.dusts.into_iter().enumerate() {
        event = event
            .add_attribute(format!("dust-token-id-{i}"), dust.token_id.to_string())
            .add_attribute(
                format!("dust-spirit-level-{i}"),
                dust.spirit_level.to_string(),
            );
    }

    event
}
