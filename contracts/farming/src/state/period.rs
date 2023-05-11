use serde::{Deserialize, Serialize};

use crate::prelude::*;

// TODO: configurable?
// 12 days
const LOCKDROP_START_DURATION: Duration = Duration::from_seconds(60 * 60 * 24 * 12);
// 2 days
const LOCKDROP_SUNSET_DURATION: Duration = Duration::from_seconds(60 * 60 * 24 * 2);

impl State<'_> {
    pub(crate) fn get_period(&self, store: &dyn Storage) -> Result<FarmingPeriod> {
        match FarmingEpoch::may_load(store)? {
            None => Ok(FarmingPeriod::Inactive),
            Some(epoch) => {
                let now = self.now();

                match epoch {
                    FarmingEpoch::Lockdrop { start } => {
                        let sunset_start = start + LOCKDROP_START_DURATION;
                        let review_start = sunset_start + LOCKDROP_SUNSET_DURATION;

                        if now < start {
                            Ok(FarmingPeriod::LockdropScheduled)
                        } else if now < sunset_start {
                            Ok(FarmingPeriod::Lockdrop)
                        } else if now < review_start {
                            Ok(FarmingPeriod::Sunset)
                        } else {
                            Ok(FarmingPeriod::Review)
                        }
                    }
                    FarmingEpoch::Launch { start } => {
                        if now < start {
                            Ok(FarmingPeriod::LaunchScheduled)
                        } else {
                            Ok(FarmingPeriod::Launched)
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn start_lockdrop_period(
        &self,
        ctx: &mut StateContext,
        start: Option<Timestamp>,
    ) -> Result<()> {
        let period = self.get_period(ctx.storage)?;

        if period != FarmingPeriod::Inactive {
            bail!(
                "Cannot start lockdrop, it has already started, currently in {:?}.",
                period
            );
        }

        let start = start.unwrap_or_else(|| self.now());
        if start < self.now() {
            bail!("Cannot start lockdrop in the past.");
        }

        FarmingEpoch::Lockdrop { start }.save(ctx.storage)?;

        Ok(())
    }

    pub(crate) fn start_launch_period(
        &self,
        ctx: &mut StateContext,
        start: Option<Timestamp>,
    ) -> Result<()> {
        let period = self.get_period(ctx.storage)?;

        if period != FarmingPeriod::Review {
            bail!(
                "Can only launch while in review period, currently in {:?}.",
                period
            );
        }

        let start = start.unwrap_or_else(|| self.now());
        if start < self.now() {
            bail!("Cannot start launch in the past.");
        }

        FarmingEpoch::Launch { start }.save(ctx.storage)?;

        Ok(())
    }

    pub(crate) fn get_schedule_countdown(&self, store: &dyn Storage) -> Result<Option<Duration>> {
        let start = match FarmingEpoch::may_load(store)? {
            None => None,
            Some(epoch) => match epoch {
                FarmingEpoch::Lockdrop { start } => Some(start),
                FarmingEpoch::Launch { start } => Some(start),
            },
        };

        match start {
            Some(start) => {
                let now = self.now();
                if start >= now {
                    Ok(Some(start.checked_sub(now, "farming schedule countdown")?))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
    pub(crate) fn get_launch_start_time(&self, store: &dyn Storage) -> Result<Timestamp> {
        match FarmingEpoch::may_load(store)? {
            None => bail!("Lockdrop has not started yet."),
            Some(epoch) => match epoch {
                FarmingEpoch::Lockdrop { .. } => bail!("Lockdrop has not finished yet."),
                FarmingEpoch::Launch { start } => Ok(start),
            },
        }
    }
}

/// The FarmingPeriod is what we really care about
/// however, it's a function of two states:
///
/// 1. Manual triggers from an admin to start the lockdrop and review
/// 2. The passage of time
///
/// So we track the manually triggered epochs *internally*
/// and then calculate the period from that
#[derive(Serialize, Deserialize, Debug)]
enum FarmingEpoch {
    Lockdrop { start: Timestamp },
    Launch { start: Timestamp },
}

impl FarmingEpoch {
    const ITEM: Item<'static, Self> = Item::new("farming-epoch");

    pub fn may_load(store: &dyn Storage) -> Result<Option<Self>> {
        Self::ITEM.may_load(store).map_err(|err| err.into())
    }

    pub fn save(&self, store: &mut dyn Storage) -> Result<()> {
        Self::ITEM.save(store, self).map_err(|err| err.into())
    }
}
