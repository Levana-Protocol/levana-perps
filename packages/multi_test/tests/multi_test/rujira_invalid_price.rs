mod helper;
use cosmwasm_std::{
    testing::{mock_env, MockApi, MockQuerier, MockStorage},
    to_json_string, Addr, OwnedDeps, Storage,
};
use helper::{setup_test_env, CustomGrpcQuerier, FACTORY_ADDR};
use levana_perpswap_multi_test::{config::SpotPriceKind, market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::market::{
        config::Config,
        spot_price::{SpotPriceConfig, SpotPriceFeed, SpotPriceFeedData, SpotPriceFeedInit},
    },
    storage::MarketId,
};
use perpswap::{
    contracts::market::{entry::NewMarketParams, spot_price::SpotPriceConfigInit},
    prelude::*,
};

#[test]
fn test_rujira() {
    let mut market = PerpsMarket::new(PerpsApp::new_cell().unwrap()).unwrap();
    let eth_rune = MarketId::new(
        "ETH",
        "RUNE",
        perpswap::storage::MarketType::CollateralIsQuote,
    );
    let rujira_feed = SpotPriceFeed {
        data: SpotPriceFeedData::Rujira {
            asset: "ETH.RUNE".to_owned(),
        },
        inverted: false,
        volatile: None,
    };
    let market_addr = market
        .exec_factory(&FactoryExecuteMsg::AddMarket {
            new_market: NewMarketParams {
                market_id: eth_rune.clone(),
                token: market.token.clone().into(),
                config: None,
                spot_price: SpotPriceConfigInit::Oracle {
                    pyth: None,
                    stride: None,
                    feeds: vec![rujira_feed.clone().into()],
                    feeds_usd: vec![rujira_feed.clone().into()],
                    volatile_diff_seconds: None,
                },
                initial_borrow_fee_rate: "0.01".parse().unwrap(),
                initial_price: None,
            },
        })
        .unwrap()
        .events
        .iter()
        .find(|e| e.ty == "instantiate")
        .context("could not instantiate")
        .unwrap()
        .attributes
        .iter()
        .find(|a| a.key == "_contract_address")
        .context("could not find contract_address")
        .unwrap()
        .value
        .clone();

    market.addr = Addr::unchecked(market_addr);
    market.id = eth_rune;

    market.exec_refresh_price().unwrap();

    let price = market.query_current_price().unwrap();
    println!("{:?}", price);
}
#[test]
fn test_get_oracle_price() {
    let rujira_feed = SpotPriceFeed {
        data: SpotPriceFeedData::Rujira {
            asset: "ETH.RUNE".to_owned(),
        },
        inverted: false,
        volatile: None,
    };

    let _spot_config = SpotPriceConfig::Oracle {
        pyth: None,
        stride: None,
        feeds: vec![rujira_feed],
        feeds_usd: Vec::new(),
        volatile_diff_seconds: None,
    };

    // Adjust this please if you like to keep it

    /*let mut deps = OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: CustomGrpcQuerier {
            base: MockQuerier::default(),
        },
        custom_query_type: PhantomData,
    };
    let env = mock_env();

    FACTORY_ADDR
        .save(
            &mut deps.storage,
            &Addr::unchecked("random address".to_owned()),
        )
        .expect("factory address initialization failed");
    deps.storage.set(
        "contract_info".as_ref(),
        r#"{
            "contract": "random contract",
            "version": "random version"
        }"#
        .as_bytes(),
    );
    deps.storage.set(
        "e".as_ref(),
        to_json_string(&Config::new(spot_config))
            .expect("Spot config is not properly setup")
            .as_bytes(),
    );

    let (state, _) = State::new(deps.as_ref(), env).expect("State is not created");

    let result = state.get_oracle_price(false);

    assert!(result.is_err());

    let error = result
        .err()
        .expect("Get oracle price should fail with zero price");
    assert_eq!(format!("{}", error), "price must be > 0");*/
}

#[test_log::test(tokio::test)]
async fn testing_basic_zero_functionability() {
    let (_app, _market, server) = setup_test_env(Some("0")).await;

    // take a look here to get familiar how works, this is my recent work:
    // https://github.com/Levana-Protocol/levana-perps/blob/main/packages/multi_test/tests/multi_test/vault.rs

    // Now we need to see how connect the market with our mock server or using your intial approach

    assert!(server.is_running(), "Server should be running...");
}

#[test_log::test(tokio::test)]
async fn testing_basic_nan_functionability() {
    let (_app, _market, server) = setup_test_env(None).await;

    // Same here

    assert!(server.is_running(), "Server should be running...");
}
