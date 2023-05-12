use serde::{Deserialize, Serialize};

use crate::prelude::*;

// TODO: configurable?
// 12 days
const LOCKDROP_START_DURATION: Duration = Duration::from_seconds(60 * 60 * 24 * 12);
// 2 days
const LOCKDROP_SUNSET_DURATION: Duration = Duration::from_seconds(60 * 60 * 24 * 2);

impl State<'_> {
    pub(crate) fn get_period(&self, store: &dyn Storage) -> Result<FarmingPeriod> {
        match FarmingEpochStartTime::may_load(store)? {
            None => Ok(FarmingPeriod::Inactive {
                lockdrop_start: None,
            }),
            Some(epoch) => {
                let now = self.now();

                match epoch {
                    FarmingEpochStartTime::Lockdrop(start) => {
                        let sunset_start = start + LOCKDROP_START_DURATION;
                        let review_start = sunset_start + LOCKDROP_SUNSET_DURATION;

                        if now < start {
                            // A scheduled lockdrop doesn't change the current period until it starts
                            Ok(FarmingPeriod::Inactive {
                                lockdrop_start: Some(start),
                            })
                        } else if now < sunset_start {
                            Ok(FarmingPeriod::Lockdrop {
                                started_at: start,
                                sunset_start,
                            })
                        } else if now < review_start {
                            Ok(FarmingPeriod::Sunset {
                                started_at: sunset_start,
                                review_start,
                            })
                        } else {
                            Ok(FarmingPeriod::Review {
                                started_at: Some(review_start),
                                launch_start: None,
                            })
                        }
                    }
                    FarmingEpochStartTime::Launch(start) => {
                        // A scheduled launch doesn't change the current period until it starts
                        if now < start {
                            Ok(FarmingPeriod::Review {
                                started_at: None,
                                launch_start: Some(start),
                            })
                        } else {
                            Ok(FarmingPeriod::Launched { started_at: start })
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

        // We allow rescheduling a lockdrop if it hasn't started yet, but not if it's already started
        if std::mem::discriminant(&period)
            != std::mem::discriminant(&FarmingPeriod::Inactive {
                lockdrop_start: None,
            })
        {
            bail!(
                "Cannot schedule a lockdrop, it has already started, currently in {:?}.",
                period
            );
        }

        let start = start.unwrap_or_else(|| self.now());
        if start < self.now() {
            bail!("Cannot start lockdrop in the past.");
        }

        FarmingEpochStartTime::Lockdrop(start).save(ctx.storage)?;

        Ok(())
    }

    pub(crate) fn start_launch_period(
        &self,
        ctx: &mut StateContext,
        start: Option<Timestamp>,
    ) -> Result<()> {
        let period = self.get_period(ctx.storage)?;

        // We allow rescheduling a launch if it hasn't started yet, but not if it's already started
        if std::mem::discriminant(&period)
            != std::mem::discriminant(&FarmingPeriod::Review {
                started_at: None,
                launch_start: None,
            })
        {
            bail!(
                "Can only launch while in review period, currently in {:?}.",
                period
            );
        }

        let start = start.unwrap_or_else(|| self.now());
        if start < self.now() {
            bail!("Cannot start launch in the past.");
        }

        FarmingEpochStartTime::Launch(start).save(ctx.storage)?;

        Ok(())
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
///
/// see get_period() for the calculation
#[derive(Serialize, Deserialize, Debug)]
enum FarmingEpochStartTime {
    Lockdrop(Timestamp),
    Launch(Timestamp),
}

impl FarmingEpochStartTime {
    const ITEM: Item<'static, Self> = Item::new("farming-epoch");

    pub fn may_load(store: &dyn Storage) -> Result<Option<Self>> {
        Self::ITEM.may_load(store).map_err(|err| err.into())
    }

    pub fn save(&self, store: &mut dyn Storage) -> Result<()> {
        Self::ITEM.save(store, self).map_err(|err| err.into())
    }
}
