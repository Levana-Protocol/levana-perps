use serde::{Serialize, Deserialize};

use crate::prelude::*;

// TODO: configurable?
// 12 days
const LOCKDROP_START_DURATION:Duration = Duration::from_seconds(60 * 60 * 24 * 12);
// 2 days
const LOCKDROP_SUNSET_DURATION:Duration = Duration::from_seconds(60 * 60 * 24 * 2);

impl State<'_> {
    pub fn get_period(&self, store: &dyn Storage) -> Result<FarmingPeriod> {
        match FarmingEpoch::may_load(store)? {
            None => Ok(FarmingPeriod::Inactive),
            Some(epoch) => {
                let now = self.now();

                match epoch {
                    FarmingEpoch::Lockdrop { start, sunset_start, review_start } => {
                        if now < start {
                            bail!("Lockdrop has both started and not started yet, that's weird!");
                        } else if now < sunset_start {
                            Ok(FarmingPeriod::Lockdrop)
                        } else if now < review_start {
                            Ok(FarmingPeriod::Sunset)
                        } else {
                            Ok(FarmingPeriod::Review)
                        }
                    },
                    FarmingEpoch::Launch => Ok(FarmingPeriod::Launched)
                }
            }
        }
    }

    pub fn start_lockdrop_period(&self, ctx: &mut StateContext) -> Result<()> {
        if self.get_period(ctx.storage)? != FarmingPeriod::Inactive {
            bail!("Lockdrop has already started.");
        }

        let start = self.now();
        let sunset_start = start + LOCKDROP_START_DURATION;
        let review_start = sunset_start + LOCKDROP_SUNSET_DURATION;

        FarmingEpoch::Lockdrop { 
            start,
            sunset_start,
            review_start
        }.save(ctx.storage)?;

        Ok(())
    }

    pub fn start_launch_period(&self, ctx: &mut StateContext) -> Result<()> {
        if self.get_period(ctx.storage)? != FarmingPeriod::Review {
            bail!("Lockdrop has not finished yet.");
        }

        FarmingEpoch::Launch.save(ctx.storage)?;

        Ok(())
    }
}

// The FarmingPeriod is what we really care about
// however, it's a function of two states:
//
// 1. Manual triggers from an admin to start the lockdrop and review
// 2. The passage of time
//
// So we track the epochs internally, and then calculate the period from that
#[derive(Serialize, Deserialize, Debug)]
enum FarmingEpoch {
    Lockdrop {
        start: Timestamp,
        sunset_start: Timestamp,
        review_start: Timestamp,
    },
    Launch
}

impl FarmingEpoch {
    const ITEM:Item<'static, Self> = Item::new("farming-epoch");

    pub fn may_load(store: &dyn Storage) -> Result<Option<Self>> {
        Self::ITEM.may_load(store).map_err(|err| err.into())
    }

    pub fn save(&self, store: &mut dyn Storage) -> Result<()> {
        Self::ITEM.save(store, self).map_err(|err| err.into())
    }
}