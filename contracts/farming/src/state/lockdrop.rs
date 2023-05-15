use crate::prelude::*;

impl State<'_> {
    pub(crate) fn lockdrop_deposit(
        &self,
        ctx: &mut StateContext,
        user: Addr,
        bucket_id: LockdropBucketId,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        LockdropBuckets::update_deposit(ctx.storage, bucket_id, &user, amount.into_number())?;
        Ok(())
    }

    pub(crate) fn lockdrop_withdraw(
        &self,
        ctx: &mut StateContext,
        user: Addr,
        bucket_id: LockdropBucketId,
        amount: NonZero<Collateral>,
    ) -> Result<()> {
        LockdropBuckets::update_deposit(ctx.storage, bucket_id, &user, -amount.into_number())?;
        Ok(())
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
                let curr = LockdropBuckets::get_deposit(storage, bucket_id, user)?;
                let half_curr = curr.into_decimal256() / (Decimal256::one() + Decimal256::one());

                if amount.into_decimal256() > half_curr {
                    bail!("can only withdraw half of the current deposit during sunset period. requested {amount}, available: {curr}");
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

impl LockdropBuckets {
    const MULTIPLIER: Map<'static, LockdropBucketId, NonZero<Decimal256>> =
        Map::new("lockdrop-bucket-multiplier");
    const DURATION: Map<'static, LockdropBucketId, Duration> = Map::new("lockdrop-bucket-duration");
    const DEPOSITS: Map<'static, (&Addr, LockdropBucketId), NonZero<Collateral>> =
        Map::new("lockdrop-bucket-deposits");

    pub fn init(storage: &mut dyn Storage, msg: &InstantiateMsg) -> Result<()> {
        for bucket in msg.lockdrop_buckets.iter() {
            let duration =
                Duration::from_seconds((bucket.bucket_id.0 * msg.lockdrop_month_seconds) as u64);

            Self::MULTIPLIER.save(storage, bucket.bucket_id, &bucket.multiplier)?;
            Self::DURATION.save(storage, bucket.bucket_id, &duration)?;
        }

        Ok(())
    }

    pub fn get_duration(storage: &dyn Storage, bucket_id: LockdropBucketId) -> Result<Duration> {
        Self::DURATION
            .load(storage, bucket_id)
            .map_err(|err| err.into())
    }

    pub fn get_deposit(
        storage: &dyn Storage,
        bucket_id: LockdropBucketId,
        user: &Addr,
    ) -> Result<NonZero<Collateral>> {
        Self::DEPOSITS
            .load(storage, (user, bucket_id))
            .map_err(|err| err.into())
    }

    pub fn update_deposit(
        storage: &mut dyn Storage,
        bucket_id: LockdropBucketId,
        user: &Addr,
        amount: Number,
    ) -> Result<()> {
        Self::DEPOSITS.update::<_, anyhow::Error>(storage, (user, bucket_id), |old| {
            let n = match old {
                Some(old) => old
                    .into_number()
                    .checked_add(amount)
                    .context("Deposit overflow")?,
                None => amount,
            };

            NonZero::<Collateral>::try_from_number(n).context("Deposit must be greater than zero")
        })?;

        Ok(())
    }
}
