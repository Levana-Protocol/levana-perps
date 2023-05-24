use super::period::FarmingPeriod;
use crate::prelude::*;
use serde::{Deserialize, Serialize};

impl State<'_> {
    pub(crate) fn lockdrop_deposit(
        &self,
        ctx: &mut StateContext,
        user: Addr,
        bucket_id: LockdropBucketId,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        let period = self.get_period(ctx.storage)?;
        LockdropBuckets::update_balance(
            ctx.storage,
            bucket_id,
            &user,
            amount.into_number(),
            period,
        )?;
        Ok(())
    }

    pub(crate) fn lockdrop_withdraw(
        &self,
        ctx: &mut StateContext,
        user: Addr,
        bucket_id: LockdropBucketId,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        let period = self.get_period(ctx.storage)?;
        LockdropBuckets::update_balance(
            ctx.storage,
            bucket_id,
            &user,
            -amount.into_number(),
            period,
        )?;

        let msg = self
            .market_info
            .collateral
            .into_transfer_msg(&user, amount)?
            .context("invalid transfer msg")?;

        ctx.response.add_message(msg);

        Ok(())
    }

    pub(crate) fn get_farmer_lockdrop_stats(
        &self,
        storage: &dyn Storage,
        farmer: &Addr,
    ) -> Result<Vec<FarmerLockdropStats>> {
        LockdropBuckets::get_all_balances_iter(storage, farmer)
            .map(|res| {
                let (bucket_id, balance) = res?;
                let total =
                    NonZero::new(balance.total()?).context("zero totals should be removed")?;

                Ok(FarmerLockdropStats {
                    bucket_id,
                    total,
                    deposit_before_sunset: balance.deposit_before_sunset,
                    deposit_after_sunset: balance.deposit_after_sunset,
                    withdrawal_before_sunset: balance.withdrawal_before_sunset,
                    withdrawal_after_sunset: balance.withdrawal_after_sunset,
                    withdrawal_after_launch: balance.withdrawal_after_launch,
                })
            })
            .collect()
    }

    pub(crate) fn validate_lockdrop_withdrawal(
        &self,
        storage: &dyn Storage,
        period_resp: &FarmingPeriodResp,
        user: &Addr,
        bucket_id: LockdropBucketId,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        match *period_resp {
            FarmingPeriodResp::Sunset { .. } => {
                let balance = LockdropBuckets::get_balance(storage, bucket_id, user)?;

                // INVARIANT: the max that can ever be withdrawn during sunset period is half_balance_before_sunset
                // multiple withdrawals accumulate in withdrawal_after_sunset, but this max is never surpassed
                // therefore the available `amount` can never be negative
                let balance_before_sunset =
                    balance.deposit_before_sunset - balance.withdrawal_before_sunset;
                let half_balance_before_sunset =
                    balance_before_sunset.into_decimal256() / Decimal256::two();
                let available =
                    half_balance_before_sunset - balance.withdrawal_after_sunset.into_decimal256();

                if amount.into_decimal256() >= available {
                    bail!("can only withdraw up to half of the original lockdrop deposit during sunset period. requested {amount}, available: {available}");
                }
            }
            FarmingPeriodResp::Launched { started_at } => {
                let ready_at = started_at + LockdropBuckets::get_duration(storage, bucket_id)?;

                if self.now() < ready_at {
                    bail!(
                        "can only withdraw after the lockdrop period is over. ready at: {ready_at}"
                    );
                }
            }
            _ => {}
        }

        Ok(())
    }
}

pub(crate) struct LockdropBuckets {}

#[derive(Serialize, Deserialize, Debug, Default)]
struct Balance {
    deposit_before_sunset: Collateral,
    deposit_after_sunset: Collateral,
    withdrawal_before_sunset: Collateral,
    withdrawal_after_sunset: Collateral,
    withdrawal_after_launch: Collateral,
}

impl Balance {
    pub fn total(&self) -> Result<Collateral> {
        let total_deposit = self.deposit_before_sunset + self.deposit_after_sunset;
        let total_withdrawal = self.withdrawal_before_sunset
            + self.withdrawal_after_sunset
            + self.withdrawal_after_launch;
        let total = total_deposit.checked_sub(total_withdrawal)?;

        Ok(total)
    }
}

impl LockdropBuckets {
    const MULTIPLIER: Map<'static, LockdropBucketId, NonZero<Decimal256>> =
        Map::new("lockdrop-bucket-multiplier");
    const DURATION: Map<'static, LockdropBucketId, Duration> = Map::new("lockdrop-bucket-duration");
    const BALANCES: Map<'static, (&Addr, LockdropBucketId), Balance> =
        Map::new("lockdrop-bucket-balances");

    pub fn init(storage: &mut dyn Storage, msg: &InstantiateMsg) -> Result<()> {
        for bucket in msg.lockdrop_buckets.iter() {
            let duration =
                Duration::from_seconds((bucket.bucket_id.0 * msg.lockdrop_month_seconds) as u64);

            Self::MULTIPLIER.save(storage, bucket.bucket_id, &bucket.multiplier)?;
            Self::DURATION.save(storage, bucket.bucket_id, &duration)?;
        }

        Ok(())
    }

    fn get_duration(storage: &dyn Storage, bucket_id: LockdropBucketId) -> Result<Duration> {
        Self::DURATION
            .load(storage, bucket_id)
            .map_err(|err| err.into())
    }

    fn get_balance(
        storage: &dyn Storage,
        bucket_id: LockdropBucketId,
        user: &Addr,
    ) -> Result<Balance> {
        Self::BALANCES
            .load(storage, (user, bucket_id))
            .map_err(|err| err.into())
    }

    fn get_all_balances_iter<'a>(
        storage: &'a dyn Storage,
        farmer: &'a Addr,
    ) -> impl Iterator<Item = Result<(LockdropBucketId, Balance)>> + 'a {
        Self::BALANCES
            .prefix(farmer)
            .range(storage, None, None, Order::Ascending)
            .map(|x| x.map_err(|err| err.into()))
    }

    fn update_balance(
        storage: &mut dyn Storage,
        bucket_id: LockdropBucketId,
        user: &Addr,
        mut amount: Number,
        period: FarmingPeriod,
    ) -> Result<()> {
        if amount.is_zero() {
            return Ok(());
        }

        let old = Self::BALANCES
            .may_load(storage, (user, bucket_id))?
            .unwrap_or_default();

        let is_withdrawal = amount.is_negative();
        if is_withdrawal {
            amount = amount.abs();
        }

        let new = match (period, is_withdrawal) {
            (FarmingPeriod::Lockdrop, true) => Balance {
                withdrawal_before_sunset: Collateral::try_from_number(
                    old.withdrawal_before_sunset
                        .into_number()
                        .checked_add(amount)
                        .context("Withdrawal overflow")?,
                )?,
                ..old
            },
            (FarmingPeriod::Sunset, true) => Balance {
                withdrawal_after_sunset: Collateral::try_from_number(
                    old.withdrawal_after_sunset
                        .into_number()
                        .checked_add(amount)
                        .context("Sunset withdrawal overflow")?,
                )?,
                ..old
            },
            (FarmingPeriod::Lockdrop, false) => Balance {
                deposit_before_sunset: Collateral::try_from_number(
                    old.deposit_before_sunset
                        .into_number()
                        .checked_add(amount)
                        .context("Deposit overflow")?,
                )?,
                ..old
            },
            (FarmingPeriod::Sunset, false) => Balance {
                deposit_after_sunset: Collateral::try_from_number(
                    old.deposit_after_sunset
                        .into_number()
                        .checked_add(amount)
                        .context("Sunset deposit overflow")?,
                )?,
                ..old
            },
            (FarmingPeriod::Launched, true) => Balance {
                withdrawal_after_launch: Collateral::try_from_number(
                    old.withdrawal_after_launch
                        .into_number()
                        .checked_add(amount)
                        .context("Withdrawal after launch overflow")?,
                )?,
                ..old
            },
            (_, false) => {
                bail!("can only deposit during lockdrop or sunset");
            }
            (_, true) => {
                bail!("can only withdraw during lockdrop, sunset, or launch");
            }
        };

        // removing depleted balances allows us to iterate over only active buckets
        // calling total() also serves as a sanity check
        if new.total()?.is_zero() {
            Self::BALANCES.remove(storage, (user, bucket_id));
        } else {
            Self::BALANCES.save(storage, (user, bucket_id), &new)?;
        }

        Ok(())
    }
}
