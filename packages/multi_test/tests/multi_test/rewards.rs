use cosmwasm_std::{Addr, Decimal256};
use levana_perpswap_multi_test::time::{BlockInfoChange, NANOS_PER_SECOND};
use levana_perpswap_multi_test::{config::TEST_CONFIG, PerpsApp};
use msg::contracts::rewards::config::Config;
use std::str::FromStr;

#[test]
fn test_distribute_rewards_unlock() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let app = &mut *app_cell.borrow_mut();
    let recipient = Addr::unchecked("recipient");

    app.setup_rewards_contract();
    app.distribute_rewards(&recipient, "100").unwrap();

    // Assert values after initial distribution

    let res = app.query_rewards_info(&recipient).unwrap();

    assert_eq!(res.locked, Decimal256::from_str("75").unwrap());
    assert_eq!(res.unlocked, Decimal256::zero());

    // Jump ahead 1/3 of the unlocking period (defaulted to 60 seconds) and assert

    let change = BlockInfoChange::from_nanos(20 * NANOS_PER_SECOND);
    app.set_block_info(change);

    let res = app.query_rewards_info(&recipient).unwrap();

    assert_eq!(res.locked, Decimal256::from_str("50").unwrap());
    assert_eq!(res.unlocked, Decimal256::from_str("25").unwrap());

    // Jump ahead to after rewards have fully unlocked

    let change = BlockInfoChange::from_nanos(40 * NANOS_PER_SECOND);
    app.set_block_info(change);

    let res = app.query_rewards_info(&recipient).unwrap();

    assert_eq!(res.locked, Decimal256::zero());
    assert_eq!(res.unlocked, Decimal256::from_str("75").unwrap());
}

#[test]
fn test_distribute_rewards_claim() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let app = &mut *app_cell.borrow_mut();
    let recipient = Addr::unchecked("recipient");

    app.setup_rewards_contract();

    // Assert error before distribution
    app.claim_rewards(&recipient).unwrap_err();

    app.distribute_rewards(&recipient, "100").unwrap();

    // Assert error after distribution but no available rewards
    app.claim_rewards(&recipient).unwrap_err();

    // Assert success after some rewards have unlocked

    let change = BlockInfoChange::from_nanos(20 * NANOS_PER_SECOND);
    app.set_block_info(change);

    let balance_before = app.query_rewards_balance(&recipient).unwrap();

    app.claim_rewards(&recipient).unwrap();

    // Assert that double claim errors out correctly
    app.claim_rewards(&recipient).unwrap_err();

    let balance_after = app.query_rewards_balance(&recipient).unwrap();
    assert_eq!(
        balance_after - balance_before,
        "25".parse::<Decimal256>().unwrap()
    );

    // Assert claim-ability of all rewards

    let change = BlockInfoChange::from_nanos(40 * NANOS_PER_SECOND);
    app.set_block_info(change);

    app.claim_rewards(&recipient).unwrap();

    let balance = app.query_rewards_balance(&recipient).unwrap();
    assert_eq!(balance, "100".parse::<Decimal256>().unwrap());
}

#[test]
fn test_multiple_distributions() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let app = &mut *app_cell.borrow_mut();
    let recipient = Addr::unchecked("recipient");

    app.setup_rewards_contract();

    // Initial distribution
    app.distribute_rewards(&recipient, "40").unwrap();

    let change = BlockInfoChange::from_nanos(20 * NANOS_PER_SECOND);
    app.set_block_info(change);

    // Second distribution
    app.distribute_rewards(&recipient, "100").unwrap();

    // Assert

    let res = app.query_rewards_info(&recipient).unwrap();

    assert_eq!(res.unlocked, Decimal256::zero());
    assert_eq!(res.locked, Decimal256::from_str("95").unwrap());

    // No need to manually claim since the second distribution triggered an automatic claim

    let balance = app.query_rewards_balance(&recipient).unwrap();
    assert_eq!(balance, "45".parse::<Decimal256>().unwrap());
}

#[test]
fn test_update_config() {
    let app_cell = PerpsApp::new_cell().unwrap();
    let app = &mut *app_cell.borrow_mut();
    let recipient = Addr::unchecked("recipient");
    let config = app.query_rewards_config().unwrap();

    app.setup_rewards_contract();

    // Initial distribution

    app.distribute_rewards(&recipient, "100").unwrap();

    let new_config = Config {
        immediately_transferable: Decimal256::from_str("0.5").unwrap(),
        ..config
    };

    // Assert err on update config with unauthorized addr
    app.update_rewards_config(Addr::unchecked("unauthorized_addr"), new_config.clone())
        .unwrap_err();

    // Assert authorized case
    app.update_rewards_config(Addr::unchecked(&TEST_CONFIG.protocol_owner), new_config)
        .unwrap();

    // Confirm update worked

    let balance_before = app.query_rewards_balance(&recipient).unwrap();
    app.distribute_rewards(&recipient, "100").unwrap();
    let balance_after = app.query_rewards_balance(&recipient).unwrap();

    // Since `immediately_transferable` was updated to 50%, the user should receive half
    // of the distribution amount immediately, as opposed to 25% as previously configured.

    assert_eq!(
        balance_after - balance_before,
        "50".parse::<Decimal256>().unwrap()
    )
}
