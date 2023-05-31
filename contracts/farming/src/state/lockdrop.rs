use super::period::FarmingPeriod;
use crate::prelude::*;
use serde::{Deserialize, Serialize};
use crate::prelude::farming::RawFarmerStats;
use crate::state::period::LockdropDurations;

//todo don't forget to set LVN_LOCKDROP_REWARDS
/// The total amount of LVN rewards designated for lockdrop participants
const LVN_LOCKDROP_REWARDS: Item<LvnToken> = Item::new(namespace::LVN_LOCKDROP_REWARDS);

impl State<'_> {
    pub(crate) fn lockdrop_init(
        &self,
        store: &mut dyn Storage,
        msg: &InstantiateMsg,
    ) -> Result<()> {
        LockdropBuckets::init(store, msg)?;
        self.save_lockdrop_rewards(store, LvnToken::zero())?;
        self.save_lockdrop_durations(store, LockdropDurations {
            start_duration: Duration::from_seconds(msg.lockdrop_start_duration.into()),
            sunset_duration: Duration::from_seconds(msg.lockdrop_sunset_duration.into()),
        })?;

        Ok(())
    }

    pub(crate) fn save_lockdrop_rewards(
        &self,
        store: &mut dyn Storage,
        amount: LvnToken,
    ) -> Result<()> {
        LVN_LOCKDROP_REWARDS.save(store, &amount)?;
        Ok(())
    }

    pub(crate) fn lockdrop_deposit(
        &self,
        ctx: &mut StateContext,
        user: Addr,
        bucket_id: LockdropBucketId,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        let period = self.get_period(ctx.storage)?;
        let mut farmer_stats = match self.load_raw_farmer_stats(ctx.storage, &user)? {
            None => RawFarmerStats::default(),
            Some(farmer_stats) => farmer_stats
        };

        let farming_tokens = FarmingToken::from_decimal256(amount.into_decimal256());
        farmer_stats.farming_tokens = farmer_stats.farming_tokens.checked_add(farming_tokens)?;
        self.save_raw_farmer_stats(ctx.storage, &user, &farmer_stats)?;

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
        user: &Addr,
    ) -> Result<Vec<FarmerLockdropStats>> {
        LockdropBuckets::get_all_balances_iter(storage, user)
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

    /// Calculates the total amount of rewards a user earned by participating in the lockdrop
    pub(crate) fn calculate_lockdrop_rewards(
        &self,
        store: &dyn Storage,
        user: &Addr,
    ) -> Result<LvnToken> {
        let user_shares = LockdropBuckets::get_shares(store, user)?;
        let total_shares = LockdropBuckets::TOTAL_SHARES.load(store)?;
        let total_rewards = LVN_LOCKDROP_REWARDS.load(store)?;
        let user_rewards = total_rewards
            .checked_mul_dec(user_shares.into_decimal256())?
            .checked_div_dec(total_shares.into_decimal256())?;

        Ok(user_rewards)
    }

    pub(crate) fn calculate_unlocked_lockdrop_rewards(
        &self,
        store: &dyn Storage,
        user: &Addr,
        stats: &RawFarmerStats
    ) -> Result<LvnToken> {
        let period = self.get_period_resp(store)?;
        let lockdrop_start = match period {
            FarmingPeriodResp::Launched { started_at } => started_at,
            _ => bail!("Cannot collect lockdrop rewards prior to launch"),
        };
        let lockdrop_config = self.load_lockdrop_config(store)?;
        let elapsed_since_start = self
            .now()
            .checked_sub(lockdrop_start, "claim_lockdrop_rewards")?;
        let total_user_rewards = self.calculate_lockdrop_rewards(store, user)?;

        let amount = if elapsed_since_start >= lockdrop_config.lockdrop_lvn_unlock_seconds {
            total_user_rewards.checked_sub(stats.lockdrop_amount_collected)?
        } else {
            let start_time = stats.lockdrop_last_collected.unwrap_or(lockdrop_start);
            let elapsed_since_last_collected = self.now().checked_sub(
                start_time,
                "claim_lockdrop_rewards, elapsed_since_last_collected",
            )?;

            let elapsed_ratio = Decimal256::from_ratio(
                elapsed_since_last_collected.as_nanos(),
                lockdrop_config.lockdrop_lvn_unlock_seconds.as_nanos(),
            );

            total_user_rewards
                .checked_mul_dec(elapsed_ratio)?
                // using min as an added precaution to make sure it never goes above the total due to rounding errors
                .min(total_user_rewards)
        };

        Ok(amount)
    }

    /// Calculates how many lockdrop shares have unlocked
    pub(crate) fn lockdrop_lockup_info(
        &self,
        store: &dyn Storage,
        addr: &Addr,
    ) -> Result<LockdropLockupInfo> {
        let lockdrop_start = match self.get_period_resp(store)? {
            FarmingPeriodResp::Launched { started_at } => started_at,
            _ => bail!("Cannot calculate unlocked lockdrop balance prior to launch"),
        };
        let elapsed = self
            .now()
            .checked_sub(lockdrop_start, "calculate_unlocked_lockdrop_balance")?;

        LockdropBuckets::get_all_balances_iter(store, addr).try_fold(
            LockdropLockupInfo::default(),
            |mut acc, res| {
                let (bucket_id, balance) = res?;
                let lockdrop_duration = LockdropBuckets::get_duration(store, bucket_id)?;
                let balance = FarmingToken::from_decimal256(balance.total()?.into_decimal256());

                acc.total = acc.total.checked_add(balance)?;

                if elapsed < lockdrop_duration {
                    acc.locked.checked_add(balance)?;
                } else {
                    acc.unlocked.checked_add(balance)?;
                }

                anyhow::Ok(acc)
            },
        )
    }
}

pub(crate) struct LockdropBuckets {}

#[derive(Default)]
/// Information about the funds deposited into the lockdrop
pub(crate) struct LockdropLockupInfo {
    /// The total amount of farming tokens that came from the lockdrop
    pub(crate) total: FarmingToken,
    /// The amount of farming tokens that came from the lockdrop that are still locked
    pub(crate) locked: FarmingToken,
    /// The amount of farming tokens that came from the lockdrop that are unlocked
    pub(crate) unlocked: FarmingToken,
}

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
        Map::new(namespace::LOCKDROP_BUCKETS_MULTIPLIER);
    const DURATION: Map<'static, LockdropBucketId, Duration> =
        Map::new(namespace::LOCKDROP_BUCKETS_DURATION);
    const BALANCES: Map<'static, (&Addr, LockdropBucketId), Balance> =
        Map::new(namespace::LOCKDROP_BUCKETS_BALANCES);
    const TOTAL_SHARES: Item<'static, LockdropShares> =
        Item::new(namespace::LOCKDROP_BUCKETS_TOTAL_SHARES);

    pub fn init(storage: &mut dyn Storage, msg: &InstantiateMsg) -> Result<()> {
        for bucket in msg.lockdrop_buckets.iter() {
            let duration =
                Duration::from_seconds((bucket.bucket_id.0 * msg.lockdrop_month_seconds) as u64);

            Self::MULTIPLIER.save(storage, bucket.bucket_id, &bucket.multiplier)?;
            Self::DURATION.save(storage, bucket.bucket_id, &duration)?;
        }

        Self::TOTAL_SHARES.save(storage, &LockdropShares::zero())?;

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
        user: &'a Addr,
    ) -> impl Iterator<Item = Result<(LockdropBucketId, Balance)>> + 'a {
        Self::BALANCES
            .prefix(user)
            .range(storage, None, None, Order::Ascending)
            .map(|x| x.map_err(|err| err.into()))
    }

    fn get_shares(store: &dyn Storage, user: &Addr) -> Result<LockdropShares> {
        Self::get_all_balances_iter(store, user)
            .try_fold(Decimal256::zero(), |acc, res| {
                let (bucket_id, balance) = res?;
                let multiplier = LockdropBuckets::MULTIPLIER
                    .load(store, bucket_id)?
                    .into_decimal256();
                let total_balance = balance.total()?.into_decimal256();
                let acc = multiplier.checked_mul(total_balance)?.checked_add(acc)?;

                anyhow::Ok(acc)
            })
            .map(LockdropShares::from_decimal256)
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

        let multiplier = LockdropBuckets::MULTIPLIER
            .load(storage, bucket_id)?
            .into_number();

        let weighted_amount = amount
            .checked_mul(multiplier)
            .map(Signed::<LockdropShares>::from_number)?;

        LockdropBuckets::TOTAL_SHARES
            .update(storage, |total| total.checked_add_signed(weighted_amount))?;

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
