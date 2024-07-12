use std::collections::HashMap;

use cosmwasm_std::Addr;
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use msg::{contracts::market::entry::ReferralStatsResp, prelude::*};

#[test]
fn no_initial_referrer() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();

    assert_eq!(
        market.query_referees(&referrer).unwrap(),
        Vec::<Addr>::new()
    );

    assert_eq!(market.query_referrer(&referee).unwrap(), None);
}

#[test]
fn register_referrer() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();

    market.exec_register_referrer(&referee, &referrer).unwrap();
    assert_eq!(
        market.query_referees(&referrer).unwrap(),
        vec![referee.clone()]
    );

    assert_eq!(market.query_referrer(&referee).unwrap(), Some(referrer));
}

#[test]
fn cannot_register_twice() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();

    market.exec_register_referrer(&referee, &referrer).unwrap();
    market
        .exec_register_referrer(&referee, &referrer)
        .unwrap_err();
}

#[test]
fn enumeration_works() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let mut referees = (1..50)
        .map(|i| market.clone_trader(i).unwrap())
        .collect::<Vec<_>>();

    for referee in &referees {
        market.exec_register_referrer(referee, &referrer).unwrap();
    }

    for referee in &referees {
        assert_eq!(
            market.query_referrer(referee).unwrap().as_ref(),
            Some(&referrer)
        );
    }

    referees.sort();

    assert_eq!(market.query_referees(&referrer).unwrap(), referees);
}

#[test]
fn no_initial_rewards() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();

    let stats = market.query_referral_stats(&referrer).unwrap();
    assert_eq!(stats, Default::default());
    market.exec_register_referrer(&referee, &referrer).unwrap();

    let lp_info = market.query_lp_info(&referrer).unwrap();
    assert_eq!(lp_info.available_referrer_rewards, Collateral::zero());
    let stats = market.query_referral_stats(&referrer).unwrap();
    assert_eq!(
        stats,
        ReferralStatsResp {
            referees: 1,
            ..Default::default()
        }
    );

    let stats = market.query_referral_stats(&referee).unwrap();
    assert_eq!(
        stats,
        ReferralStatsResp {
            referrer: Some(referrer.clone()),
            ..Default::default()
        }
    );
}

#[test]
fn rewards_for_registered() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let referrer = market.clone_trader(0).unwrap();
    let referee = market.clone_trader(1).unwrap();
    let other = market.clone_trader(2).unwrap();

    let stats = market.query_referral_stats(&referrer).unwrap();
    assert_eq!(stats, Default::default());
    market.exec_register_referrer(&referee, &referrer).unwrap();

    let lp_info = market.query_lp_info(&referrer).unwrap();
    assert_eq!(lp_info.available_referrer_rewards, Collateral::zero());
    let stats = market.query_referral_stats(&referrer).unwrap();
    assert_eq!(
        stats,
        ReferralStatsResp {
            referees: 1,
            ..Default::default()
        }
    );

    market
        .exec_open_position(
            &other,
            "5",
            "2.5",
            DirectionToBase::Long,
            "2.1",
            None,
            None,
            None,
        )
        .unwrap();

    let lp_info = market.query_lp_info(&referrer).unwrap();
    assert_eq!(lp_info.available_referrer_rewards, Collateral::zero());
    let stats = market.query_referral_stats(&referrer).unwrap();
    assert_eq!(
        stats,
        ReferralStatsResp {
            referees: 1,
            ..Default::default()
        }
    );
    let stats = market.query_referral_stats(&referee).unwrap();
    assert_eq!(
        stats,
        ReferralStatsResp {
            referrer: Some(referrer.clone()),
            ..Default::default()
        }
    );

    let (pos_id, _) = market
        .exec_open_position(
            &referee,
            "5",
            "2.5",
            DirectionToBase::Long,
            "2.1",
            None,
            None,
            None,
        )
        .unwrap();
    let pos = market.query_position(pos_id).unwrap();
    let config = market.query_config().unwrap();

    let lp_info = market.query_lp_info(&referrer).unwrap();
    assert_eq!(
        lp_info.available_referrer_rewards,
        pos.trading_fee_collateral
            .checked_mul_dec(config.referral_reward_ratio)
            .unwrap()
    );

    let recv_stats = market.query_referral_stats(&referrer).unwrap();
    assert_eq!(recv_stats.received, lp_info.available_referrer_rewards);
    assert_eq!(recv_stats.generated, Collateral::zero());

    let send_stats = market.query_referral_stats(&referee).unwrap();
    assert_eq!(send_stats.generated, lp_info.available_referrer_rewards);
    assert_eq!(send_stats.received, Collateral::zero());

    market.exec_claim_yield(&referrer).unwrap();

    let lp_info_final = market.query_lp_info(&referrer).unwrap();
    assert_eq!(lp_info_final.available_referrer_rewards, Collateral::zero());
    let stats = market.query_referral_stats(&referrer).unwrap();
    assert_eq!(stats.received, lp_info.available_referrer_rewards);
    assert_ne!(stats.received_usd, Usd::zero());
}

#[test]
fn referee_count() {
    let market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();

    let mut next_referee = 0;
    let mut referrers = HashMap::new();
    for count in 5..=11 {
        let referrer = market.clone_lp(count).unwrap();
        for _ in 0..count {
            let referee = market.clone_trader(next_referee).unwrap();
            next_referee += 1;
            market.exec_register_referrer(&referee, &referrer).unwrap();
        }
        let stats = market.query_referral_stats(&referrer).unwrap();
        assert_eq!(stats.referees, stats.referees);

        referrers.insert(referrer, u32::try_from(count).unwrap());
    }

    assert_eq!(market.query_referrer_counts().unwrap(), referrers);
}
