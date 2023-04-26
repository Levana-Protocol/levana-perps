use levana_perpswap_multi_test::arbitrary::position_open::{
    data::PositionOpen, runner::OpenExpect,
};
use proptest::prelude::*;

proptest! {
    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_open(
        strategy in PositionOpen::new_strategy()
    ) {
        strategy.run(OpenExpect::Success).unwrap();
    }
}
