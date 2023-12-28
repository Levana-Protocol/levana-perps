use cosmwasm_std::{BlockInfo, Timestamp};

const SECS_PER_BLOCK: i64 = 7;
pub const NANOS_PER_SECOND: i64 = 1_000_000_000;
const NANOS_PER_BLOCK: i64 = SECS_PER_BLOCK * NANOS_PER_SECOND;

// Encapsulates simulated time jumps (i.e. moves time and block height together)
// use with PerpsApp::set_block_time_raw
// or just pass to PerpsMarket
#[derive(Debug, Clone, Copy)]
pub enum TimeJump {
    Nanos(i64),
    Seconds(i64),
    Minutes(i64),
    Hours(i64),
    Liquifundings(i64),
    Blocks(i64),
    FractionalLiquifundings(f64),
    PreciseTime(Timestamp),
    PreciseHeight(u64),
}

pub struct BlockInfoChange {
    pub height: i64,
    pub nanos: i64,
}

impl BlockInfoChange {
    pub fn from_nanos(nanos: i64) -> Self {
        // taken from https://github.com/rust-lang/rust/pull/88582/files#diff-dd440fe33121a785308d5cde98a1ab79b0b285d27bb29eaa9800e180870e16a6R1788
        // but adapted slightly, since we want to ceil away from 0 (i.e. the ceil of a negative should be "more negative")
        const fn signed_div_ceil(a: i64, b: i64) -> i64 {
            let sign = match (a >= 0, b >= 0) {
                (true, true) => 1,
                (false, false) => 1,
                (true, false) => -1,
                (false, true) => -1,
            };

            let a = a.unsigned_abs();
            let b = b.unsigned_abs();

            let d = a / b;
            let r = a % b;
            let res = if r > 0 && b > 0 { d + 1 } else { d };

            res as i64 * sign
        }
        Self {
            height: signed_div_ceil(nanos, NANOS_PER_BLOCK),
            nanos,
        }
    }

    pub(crate) fn from_time_jump(
        time_jump: TimeJump,
        current_block_info: BlockInfo,
        liquifunding_duration: u64,
    ) -> Self {
        let nanos = match time_jump {
            TimeJump::Nanos(n) => n,
            TimeJump::Seconds(n) => n * NANOS_PER_SECOND,
            TimeJump::Minutes(n) => n * 60 * NANOS_PER_SECOND,
            TimeJump::Hours(n) => n * 60 * 60 * NANOS_PER_SECOND,
            TimeJump::Liquifundings(n) => n * liquifunding_duration as i64 * NANOS_PER_SECOND,
            TimeJump::FractionalLiquifundings(n) => {
                ((n * liquifunding_duration as f64) * NANOS_PER_SECOND as f64) as i64
            }
            TimeJump::Blocks(n) => n * SECS_PER_BLOCK * NANOS_PER_SECOND,
            TimeJump::PreciseTime(n) => n.nanos() as i64 - current_block_info.time.nanos() as i64,
            TimeJump::PreciseHeight(n) => {
                (n as i64 - current_block_info.height as i64) * SECS_PER_BLOCK * NANOS_PER_SECOND
            }
        };

        Self::from_nanos(nanos)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::PerpsApp;
    #[test]
    fn time_jump_math() {
        let app = PerpsApp::new().unwrap();
        let assert_height = |time_jump: TimeJump, expected_height: i64| {
            let change = BlockInfoChange::from_time_jump(time_jump, app.block_info(), 3600);

            if change.height != expected_height {
                panic!(
                    "height of {} != {} for time jump {:?}",
                    change.height, expected_height, time_jump
                );
            }
        };

        for sign in [1i64, -1i64] {
            assert_height(TimeJump::Seconds(0), 0);
            assert_height(TimeJump::Seconds(3 * sign), sign);
            assert_height(TimeJump::Seconds(7 * sign), sign);
            assert_height(TimeJump::Seconds(8 * sign), 2 * sign);
            assert_height(TimeJump::Seconds(10 * sign), 2 * sign);
            assert_height(TimeJump::Seconds(14 * sign), 2 * sign);
            assert_height(TimeJump::Seconds(15 * sign), 3 * sign);
            assert_height(TimeJump::Nanos(8_000_000_000 * sign), 2 * sign);
            assert_height(TimeJump::Minutes(2 * sign), 18 * sign);
            assert_height(TimeJump::Liquifundings(2 * sign), 1029 * sign);
            assert_height(
                TimeJump::FractionalLiquifundings(0.5 * sign as f64),
                258 * sign,
            );
            assert_height(TimeJump::Blocks(0), 0);
            assert_height(TimeJump::Blocks(3 * sign), 3 * sign);
            assert_height(TimeJump::Blocks(49 * sign), 49 * sign);
            if sign > 0 {
                assert_height(
                    TimeJump::PreciseTime(app.block_info().time.plus_seconds(10)),
                    2,
                );
                assert_height(TimeJump::PreciseHeight(app.block_info().height + 2), 2);
            } else {
                assert_height(
                    TimeJump::PreciseTime(app.block_info().time.minus_seconds(10)),
                    -2,
                );
                assert_height(TimeJump::PreciseHeight(app.block_info().height - 2), -2);
            }
        }
    }
}
