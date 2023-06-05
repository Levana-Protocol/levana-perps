use crate::arbitrary::farming::emissions::data::Action;

use super::data::FarmingEmissions;
use anyhow::Result;
use msg::prelude::Collateral;

impl FarmingEmissions {
    pub fn run(&self) -> Result<()> {

        println!("{:#?}", self);
        // let mut market = self.market.borrow_mut();
        // market.automatic_time_jump_enabled = false;
        // let lp = market.clone_lp(0).unwrap();

        // let total_collateral_deposit = self.actions.iter().fold(Collateral::zero(), |acc, action| {
        //     match action {
        //         Action::Deposit { collateral, .. } => acc + collateral,

        //     }
        //     if let Some(deposit) = action.deposit {
        //         acc + deposit
        //     } else {
        //         acc
        //     }
        // });
        // market
        //     .exec_mint_and_deposit_liquidity(&lp, "100".parse().unwrap())
        //     .unwrap();
        // market.exec_stake_lp(&lp, None).unwrap();
    
        // market.exec_farming_start_lockdrop(None).unwrap();
        // market.set_time(TimeJump::Hours(24 * 365)).unwrap();
        // market.exec_farming_start_launch(None).unwrap();
    
        // let amount = "200";
        // let token = market.setup_lvn_rewards(amount);
    
        // // sanity check
        // let protocol_owner = Addr::unchecked(&TEST_CONFIG.protocol_owner);
        // let balance = market.query_reward_token_balance(&token, &protocol_owner);
        // assert_eq!(balance, LvnToken::from_str(amount).unwrap());
    
        // market
        //     .exec_farming_set_emissions(market.now(), 20, amount.parse().unwrap(), token)
        //     .unwrap();
    
        // // Test query farming rewards
    
        // market
        //     .exec_farming_deposit_xlp(&lp, NonZero::new("100".parse().unwrap()).unwrap())
        //     .unwrap();
    
        // market.set_time(TimeJump::Seconds(5)).unwrap();
        // let stats = market.query_farming_farmer_stats(&lp).unwrap();
        // assert_eq!(stats.emission_rewards, "50".parse().unwrap());
    
        // market.set_time(TimeJump::Seconds(15)).unwrap();
        // let stats = market.query_farming_farmer_stats(&lp).unwrap();
        // assert_eq!(stats.emission_rewards, "200".parse().unwrap());
        Ok(())
    }
}