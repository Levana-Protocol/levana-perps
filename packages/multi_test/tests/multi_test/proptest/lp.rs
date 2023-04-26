use levana_perpswap_multi_test::arbitrary::lp::data::{
    LpDepositWithdraw, LpYield, XlpStakeUnstake,
};
use proptest::prelude::*;

proptest! {
    /*
    #![proptest_config(ProptestConfig{
        failure_persistence: None,
        max_shrink_iters: 0,
        max_local_rejects: 1,
        max_global_rejects: 1,
        .. ProptestConfig::with_cases(10)
    })]
    */

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_lp(
        strategy in LpDepositWithdraw::new_strategy()
    ) {
        strategy.run().unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_xlp(
        strategy in XlpStakeUnstake::new_strategy()
    ) {
        strategy.run().unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_lp_yield(
        strategy in LpYield::new_strategy()
    ) {
        strategy.run().unwrap();
    }
}
