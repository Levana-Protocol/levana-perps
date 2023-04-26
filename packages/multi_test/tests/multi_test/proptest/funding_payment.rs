use levana_perpswap_multi_test::arbitrary::funding_payment::{
    data::FundingPayment, runner::FundingPaymentExpect,
};
use proptest::prelude::*;

proptest! {
    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_funding_payment(
        strategy in FundingPayment::new_strategy()
    ) {
        strategy.run(FundingPaymentExpect::Success).unwrap();
    }
}
