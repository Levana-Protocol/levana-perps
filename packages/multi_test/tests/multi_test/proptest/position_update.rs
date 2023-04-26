use levana_perpswap_multi_test::arbitrary::position_update::{
    data::{
        PositionUpdateAddCollateralImpactLeverage, PositionUpdateAddCollateralImpactSize,
        PositionUpdateLeverage, PositionUpdateMaxGains,
        PositionUpdateRemoveCollateralImpactLeverage, PositionUpdateRemoveCollateralImpactSize,
    },
    runner::UpdateExpect,
};
use proptest::prelude::*;

proptest! {
    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_add_collateral_leverage(
        strategy in PositionUpdateAddCollateralImpactLeverage::new_strategy()
    ) {
        strategy.run(UpdateExpect::Success).unwrap();

    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_add_collateral_size(
        strategy in PositionUpdateAddCollateralImpactSize::new_strategy()
    ) {
        strategy.run(UpdateExpect::Success).unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_add_collateral_size_valid_slippage(
        strategy in PositionUpdateAddCollateralImpactSize::new_strategy_valid_slippage()
    ) {
        strategy.run(UpdateExpect::Success).unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_add_collateral_size_exceed_slippage(
        strategy in PositionUpdateAddCollateralImpactSize::new_strategy_exceed_slippage()
    ) {
        strategy.run(UpdateExpect::FailSlippage).unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_remove_collateral_leverage(
        strategy in PositionUpdateRemoveCollateralImpactLeverage::new_strategy()
    ) {
        strategy.run(UpdateExpect::Success).unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_remove_collateral_size_valid_slippage(
        strategy in PositionUpdateRemoveCollateralImpactSize::new_strategy_valid_slippage()
    ) {
        strategy.run(UpdateExpect::Success).unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_remove_collateral_size_exceed_slippage(
        strategy in PositionUpdateRemoveCollateralImpactSize::new_strategy_exceed_slippage()
    ) {
        strategy.run(UpdateExpect::FailSlippage).unwrap();
    }


    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_remove_collateral_size(
        strategy in PositionUpdateRemoveCollateralImpactSize::new_strategy()
    ) {
        strategy.run(UpdateExpect::Success).unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_leverage(
        strategy in PositionUpdateLeverage::new_strategy()
    ) {
        strategy.run(UpdateExpect::Success).unwrap();
    }

    #[test]
    #[cfg_attr(not(feature = "proptest"), ignore)]
    fn proptest_position_update_max_gains(
        strategy in PositionUpdateMaxGains::new_strategy()
    ) {
        strategy.run(UpdateExpect::Success).unwrap();
    }
}
