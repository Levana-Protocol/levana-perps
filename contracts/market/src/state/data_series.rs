use crate::prelude::*;
use cosmwasm_std::{Order, Storage};
use cw_storage_plus::{Bound, Map};
use serde::{Deserialize, Serialize};

/// A DataPoint encapsulates a given value as well as the sum of all values up until this DataPoint
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DataPoint {
    prefix_sum: Number,
    pub value: Number,
}

/// A DataSeries tracks values over time and allows for efficient operations over a given range in the series
pub(crate) struct DataSeries<'a> {
    map: Map<'a, Timestamp, DataPoint>,
}

impl<'a> DataSeries<'a> {
    pub(crate) const fn new(namespace: &'a str) -> Self {
        DataSeries {
            map: Map::new(namespace),
        }
    }

    /// Attempt to load the last [DataPoint]
    pub(crate) fn try_load_last(
        &self,
        storage: &dyn Storage,
    ) -> Result<Option<(Timestamp, DataPoint)>> {
        let last = self
            .map
            .range(storage, None, None, Order::Descending)
            .next()
            .transpose()?;

        Ok(last)
    }

    /// Appends specified value to the [DataSeries].
    /// If a data point already exists at the specified timestamp, it will be overriden.
    pub(crate) fn append(
        &self,
        storage: &mut dyn Storage,
        time: Timestamp,
        value: Number,
    ) -> Result<()> {
        let last = self.try_load_last(storage)?;
        let prefix_sum = match last {
            None => Number::ZERO,
            Some((last_time, data_point)) => {
                let elapsed =
                    Number::from((time.checked_sub(last_time, "DataSeries::append")?).as_nanos());
                let elapsed_sum = data_point.value.checked_mul(elapsed)?;
                data_point.prefix_sum.checked_add(elapsed_sum)?
            }
        };
        let data_point = DataPoint { prefix_sum, value };

        self.map.save(storage, time, &data_point)?;
        Ok(())
    }

    /// Returns the sum of [DataPoint] from start to end per nanosecond
    pub(crate) fn sum(
        &self,
        storage: &dyn Storage,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Number> {
        let (start_data_point_time, start_data_point) = self
            .map
            .range(
                storage,
                None,
                Some(Bound::inclusive(start)),
                Order::Descending,
            )
            .next()
            .transpose()?
            .with_context(|| format!("Unable to find entry for start time {}", start))?;

        let (end_data_point_time, end_data_point) = self
            .map
            .range(
                storage,
                None,
                Some(Bound::exclusive(end)),
                Order::Descending,
            )
            .next()
            .transpose()?
            .with_context(|| format!("Unable to find entry for end time {}", end))?;

        let elapsed_start: Number = (start
            .checked_sub(start_data_point_time, "DataSeries::sum, elapsed_start")?)
        .as_nanos()
        .into();
        let elapsed_sum = start_data_point.value.checked_mul(elapsed_start)?;
        let start_prefix_sum = start_data_point.prefix_sum.checked_add(elapsed_sum)?;

        let elapsed_end: Number = (end
            .checked_sub(end_data_point_time, "DataSeries::sum, elapsed_end")?)
        .as_nanos()
        .into();
        let elapsed_sum = end_data_point.value.checked_mul(elapsed_end)?;
        let end_prefix_sum = end_data_point.prefix_sum.checked_add(elapsed_sum)?;

        end_prefix_sum.checked_sub(start_prefix_sum)
    }
}
