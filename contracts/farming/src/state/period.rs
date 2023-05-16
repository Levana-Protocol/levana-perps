use serde::{Deserialize, Serialize};

use crate::prelude::*;

// TODO: configurable?
// 12 days
const LOCKDROP_START_DURATION: Duration = Duration::from_seconds(60 * 60 * 24 * 12);
// 2 days
const LOCKDROP_SUNSET_DURATION: Duration = Duration::from_seconds(60 * 60 * 24 * 2);

// Almost all the times flow naturally from the epoch timestamps
// Review start time is an exception, so we stash it
const REVIEW_START_TIME: Item<Timestamp> = Item::new("review-start-time");

// The current farming period, without the baggage of FarmingPeriodResp
// used for internal contract logic only
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FarmingPeriod {
    Inactive,
    Lockdrop,
    Sunset,
    Review,
    Launched,
}

impl From<FarmingPeriodResp> for FarmingPeriod {
    fn from(resp: FarmingPeriodResp) -> Self {
        match resp {
            FarmingPeriodResp::Inactive { .. } => FarmingPeriod::Inactive,
            FarmingPeriodResp::Lockdrop { .. } => FarmingPeriod::Lockdrop,
            FarmingPeriodResp::Sunset { .. } => FarmingPeriod::Sunset,
            FarmingPeriodResp::Review { .. } => FarmingPeriod::Review,
            FarmingPeriodResp::Launched { .. } => FarmingPeriod::Launched,
        }
    }
}

impl State<'_> {
    pub(crate) fn validate_period_msg(&self, store: &dyn Storage, msg: &ExecuteMsg) -> Result<()> {
        let period = self.get_period(store)?;

        let is_valid = match msg {
            ExecuteMsg::Owner(_) => true,
            ExecuteMsg::Receive { .. } => {
                anyhow::bail!("Cannot have double-wrapped Receive");
            }
            ExecuteMsg::LockdropDeposit { .. } => {
                period == FarmingPeriod::Lockdrop || period == FarmingPeriod::Sunset
            }
            ExecuteMsg::LockdropWithdraw { .. } => {
                match period {
                    FarmingPeriod::Lockdrop => true,
                    FarmingPeriod::Sunset => {
                        // TODO - check that amount is no more than half the bucket
                        true
                    }
                    FarmingPeriod::Launched => {
                        // TODO - check that the lockdrop has finished for this bucket
                        true
                    }
                    _ => false,
                }
            }
            ExecuteMsg::Deposit { .. }
            | ExecuteMsg::Withdraw { .. }
            | ExecuteMsg::ClaimLvn {}
            | ExecuteMsg::Reinvest {}
            | ExecuteMsg::TransferBonus {} => period == FarmingPeriod::Launched,
        };

        if !is_valid {
            Err(anyhow::anyhow!("Not allowed during {:?}", period))
        } else {
            Ok(())
        }
    }

    pub(crate) fn get_period(&self, store: &dyn Storage) -> Result<FarmingPeriod> {
        self.get_period_resp(store).map(Into::into)
    }

    pub(crate) fn get_period_resp(&self, store: &dyn Storage) -> Result<FarmingPeriodResp> {
        match FarmingEpochStartTime::may_load(store)? {
            None => Ok(FarmingPeriodResp::Inactive {
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
                            Ok(FarmingPeriodResp::Inactive {
                                lockdrop_start: Some(start),
                            })
                        } else if now < sunset_start {
                            Ok(FarmingPeriodResp::Lockdrop {
                                started_at: start,
                                sunset_start,
                            })
                        } else if now < review_start {
                            Ok(FarmingPeriodResp::Sunset {
                                started_at: sunset_start,
                                review_start,
                            })
                        } else {
                            Ok(FarmingPeriodResp::Review {
                                started_at: review_start,
                                launch_start: None,
                            })
                        }
                    }
                    FarmingEpochStartTime::Launch(start) => {
                        // A scheduled launch doesn't change the current period until it starts
                        if now < start {
                            Ok(FarmingPeriodResp::Review {
                                // in this case we can't calculate the review start time
                                // rather, we use it from the stashed value
                                // which definitively exists if we're in this branch
                                started_at: REVIEW_START_TIME.load(store)?,
                                launch_start: Some(start),
                            })
                        } else {
                            Ok(FarmingPeriodResp::Launched { started_at: start })
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
        if period != FarmingPeriod::Inactive {
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
        let period_resp = self.get_period_resp(ctx.storage)?;
        match period_resp {
            FarmingPeriodResp::Review { started_at, .. } => {
                // this will remain the consistent review start time until the launch starts
                // but it's perhaps a bit more optimal to only save if we need to
                // since writes are probably more expensive than reads
                // and it's a bit easier to see that it stays consistent this way
                if REVIEW_START_TIME.may_load(ctx.storage)?.is_none() {
                    REVIEW_START_TIME.save(ctx.storage, &started_at)?;
                }

                let start = start.unwrap_or_else(|| self.now());
                if start < self.now() {
                    bail!("Cannot start launch in the past.");
                }

                FarmingEpochStartTime::Launch(start).save(ctx.storage)?;

                Ok(())
            }
            _ => {
                bail!(
                    "Can only launch while in review period, currently in {:?}.",
                    FarmingPeriod::from(period_resp)
                );
            }
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
///
/// see get_period() for the calculation, which also takes into account
/// scheduled changes (i.e. where the epoch timestamp is in the future)
///
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
