use anyhow::{anyhow, Result};
use cosmwasm_std::{
    testing::{MockApi, MockQuerier, MockStorage},
    to_json_vec, Addr, Api, Binary, BlockInfo, ContractResult, Decimal, Empty, GrpcQuery, Querier,
    QueryRequest, Storage, SystemError, SystemResult,
};
use cw_multi_test::{no_init, App, AppBuilder, BankKeeper, Module};
use levana_perpswap_multi_test::{market_wrapper::PerpsMarket, PerpsApp};
use perpswap::{
    contracts::market::{
        config::ConfigUpdate,
        spot_price::{SpotPriceConfigInit, SpotPriceFeedDataInit, SpotPriceFeedInit},
    },
    prelude::*,
    price::PriceBaseInQuote,
};
use prost::Message;
use rujira_rs::proto::types::{QueryPoolRequest, QueryPoolResponse};
use std::{cell::RefCell, rc::Rc};

// This test verifies that when the Rujira oracle returns a zero price,
// the system doesn't liquidate positions incorrectly.

#[test]
fn test_rujira_zero_price_handling() {
    // Try to parse a very small price string
    let very_small_price_str = "0.0000000000000000000000000001";
    let parse_result = very_small_price_str.parse::<PriceBaseInQuote>();

    // This should fail because the price is too small to be parsed as a non-zero value
    assert!(parse_result.is_err());

    // Try to parse a zero price string
    let zero_price_str = "0";
    let parse_result = zero_price_str.parse::<PriceBaseInQuote>();

    // This should fail because the price must be > 0
    assert!(parse_result.is_err());

    // Try to create a zero Decimal
    let zero_decimal = Decimal::zero();

    // Try to create a PriceBaseInQuote from a zero Decimal
    let result = PriceBaseInQuote::try_from(zero_decimal);

    // This should fail because the price must be > 0
    assert!(result.is_err());
}

// Custom module that intercepts gRPC queries and returns zero or NaN prices
#[derive(Default)]
pub struct ZeroPriceModule {
    pub price_value: String, // "0" or "NaN"
}

impl Module for ZeroPriceModule {
    type ExecT = Empty;
    type QueryT = Empty;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn cw_multi_test::CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _sender: Addr,
        _msg: Self::ExecT,
    ) -> anyhow::Result<cw_multi_test::AppResponse>
    where
        ExecC: cosmwasm_std::CustomMsg + serde::de::DeserializeOwned + 'static,
        QueryC: cosmwasm_std::CustomQuery + serde::de::DeserializeOwned + 'static,
    {
        Err(anyhow!("Execute not implemented for ZeroPriceModule"))
    }

    fn query(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: Self::QueryT,
    ) -> anyhow::Result<Binary> {
        // Create a custom querier that will handle the request
        let custom_querier = ZeroPriceGrpcQuerier {
            base: MockQuerier::default(),
            price_value: self.price_value.clone(),
        };

        // Convert the request to JSON and pass it to the custom querier
        match custom_querier.raw_query(&to_json_vec(&request)?) {
            SystemResult::Ok(ContractResult::Ok(res)) => Ok(res),
            SystemResult::Ok(ContractResult::Err(err)) => Err(anyhow!(err)),
            SystemResult::Err(err) => Err(anyhow!(err)),
        }
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn cw_multi_test::CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> anyhow::Result<cw_multi_test::AppResponse>
    where
        ExecC: cosmwasm_std::CustomMsg + serde::de::DeserializeOwned + 'static,
        QueryC: cosmwasm_std::CustomQuery + serde::de::DeserializeOwned + 'static,
    {
        Err(anyhow!("Sudo not implemented for ZeroPriceModule"))
    }
}

// Custom querier that returns zero or NaN prices for Rujira queries
pub struct ZeroPriceGrpcQuerier {
    pub base: MockQuerier,
    pub price_value: String, // "0" or "NaN"
}

impl Querier for ZeroPriceGrpcQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> cosmwasm_std::QuerierResult {
        let request: QueryRequest<Empty> = match cosmwasm_std::from_json(bin_request) {
            Ok(req) => req,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: e.to_string(),
                    request: Binary::from(bin_request),
                })
            }
        };

        match request {
            QueryRequest::Grpc(GrpcQuery { path, data }) => {
                if path == "/types.Query/Pool" {
                    let req =
                        QueryPoolRequest::decode(&*data.to_vec()).expect("Request body is invalid");
                    let mock_response = QueryPoolResponse {
                        asset: req.asset,
                        asset_tor_price: self.price_value.clone(),
                        status: "Available".to_owned(),
                        // Other fields with default values
                        pending_inbound_asset: "1".to_owned(),
                        pending_inbound_rune: "1".to_owned(),
                        balance_asset: "1".to_owned(),
                        balance_rune: "1".to_owned(),
                        pool_units: "1".to_owned(),
                        lp_units: "1".to_owned(),
                        synth_units: "1".to_owned(),
                        synth_supply: "1".to_owned(),
                        savers_depth: "1".to_owned(),
                        savers_units: "1".to_owned(),
                        savers_fill_bps: "1".to_owned(),
                        savers_capacity_remaining: "1".to_owned(),
                        synth_supply_remaining: "1".to_owned(),
                        loan_collateral: "1".to_owned(),
                        loan_collateral_remaining: "1".to_owned(),
                        loan_cr: "1".to_owned(),
                        derived_depth_bps: "1".to_owned(),
                        ..Default::default()
                    };

                    let mut buf = Vec::new();
                    if mock_response.encode(&mut buf).is_err() {
                        return SystemResult::Err(SystemError::InvalidResponse {
                            error: "Response encode error".to_owned(),
                            response: Binary::new(vec![]),
                        });
                    }

                    SystemResult::Ok(ContractResult::Ok(Binary::from(buf)))
                } else {
                    // Pass through other gRPC queries
                    self.base.raw_query(bin_request)
                }
            }
            // Pass through non-gRPC queries
            _ => self.base.raw_query(bin_request),
        }
    }
}

// Helper function to create a custom App with a zero or NaN price
pub fn create_app_with_custom_price(
    price_value: String,
) -> App<BankKeeper, MockApi, MockStorage, ZeroPriceModule> {
    AppBuilder::new()
        .with_custom(ZeroPriceModule { price_value })
        .build(no_init)
}

// Helper function to create a PerpsApp with a custom App that returns zero or NaN prices
pub fn create_perps_app_with_custom_price(price_value: String) -> Result<Rc<RefCell<PerpsApp>>> {
    // Create a custom App with the specified price value
    let app = create_app_with_custom_price(price_value);

    // Create a PerpsApp with the custom App
    let perps_app = PerpsApp::new_with_custom_app(app)?;

    Ok(Rc::new(RefCell::new(perps_app)))
}

#[test]
fn test_market_with_zero_price() {
    // Create a PerpsApp with a zero price
    let perps_app = create_perps_app_with_custom_price("0".to_string()).unwrap();

    // Create a market with the custom App
    let market = PerpsMarket::new(perps_app).unwrap();
    let trader = market.clone_trader(0).unwrap();

    // Set initial price (using manual price setting)
    market.exec_set_price("10".parse().unwrap()).unwrap();

    // Open a long position
    let (long_position_id, _) = market
        .exec_open_position(
            &trader,
            "100", // Collateral
            "5",   // Leverage
            DirectionToBase::Long,
            "1.0", // Slippage tolerance
            None,
            None,
            None,
        )
        .unwrap();

    // Verify the position exists
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 1);

    // Get the initial position details
    let initial_position = market.query_position(long_position_id).unwrap();

    // Configure the market to use Rujira oracle
    let rujira_feed = SpotPriceFeedInit {
        data: SpotPriceFeedDataInit::Rujira {
            asset: "ETH.RUNE".to_owned(),
        },
        inverted: false,
        volatile: None,
    };

    let spot_config = SpotPriceConfigInit::Oracle {
        pyth: None,
        stride: None,
        feeds: vec![rujira_feed],
        feeds_usd: Vec::new(),
        volatile_diff_seconds: None,
    };

    // Update the market to use the Rujira oracle
    market
        .exec_set_config(ConfigUpdate {
            spot_price: Some(spot_config),
            ..Default::default()
        })
        .unwrap();

    // Try to refresh the price, which should trigger a query to the Rujira oracle
    // This should fail because the price is zero
    let result = market.exec_refresh_price();

    // Verify that the refresh failed with the expected error
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("price must be > 0"),
        "Expected error to contain 'price must be > 0', got: {}",
        error
    );

    // Verify that the position still exists and hasn't been liquidated
    let positions_after = market.query_positions(&trader).unwrap();
    assert_eq!(positions_after.len(), 1);

    // Verify that the position details haven't changed
    let position_after = market.query_position(long_position_id).unwrap();
    assert_eq!(initial_position, position_after);
}

#[test]
fn test_market_with_nan_price() {
    // Create a PerpsApp with a NaN price
    let perps_app = create_perps_app_with_custom_price("NaN".to_string()).unwrap();

    // Create a market with the custom App
    let market = PerpsMarket::new(perps_app).unwrap();
    let trader = market.clone_trader(0).unwrap();

    // Set initial price (using manual price setting)
    market.exec_set_price("10".parse().unwrap()).unwrap();

    // Open a long position
    let (long_position_id, _) = market
        .exec_open_position(
            &trader,
            "100", // Collateral
            "5",   // Leverage
            DirectionToBase::Long,
            "1.0", // Slippage tolerance
            None,
            None,
            None,
        )
        .unwrap();

    // Verify the position exists
    let positions = market.query_positions(&trader).unwrap();
    assert_eq!(positions.len(), 1);

    // Get the initial position details
    let initial_position = market.query_position(long_position_id).unwrap();

    // Configure the market to use Rujira oracle
    let rujira_feed = SpotPriceFeedInit {
        data: SpotPriceFeedDataInit::Rujira {
            asset: "ETH.RUNE".to_owned(),
        },
        inverted: false,
        volatile: None,
    };

    let spot_config = SpotPriceConfigInit::Oracle {
        pyth: None,
        stride: None,
        feeds: vec![rujira_feed],
        feeds_usd: Vec::new(),
        volatile_diff_seconds: None,
    };

    // Update the market to use the Rujira oracle
    market
        .exec_set_config(ConfigUpdate {
            spot_price: Some(spot_config),
            ..Default::default()
        })
        .unwrap();

    // Try to refresh the price, which should trigger a query to the Rujira oracle
    // This should fail because the price is NaN
    let result = market.exec_refresh_price();

    // Verify that the refresh failed with the expected error
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(
        error.to_string().contains("price must be > 0")
            || error.to_string().contains("invalid digit found in string"),
        "Expected error to contain 'price must be > 0' or 'invalid digit found in string', got: {}",
        error
    );

    // Verify that the position still exists and hasn't been liquidated
    let positions_after = market.query_positions(&trader).unwrap();
    assert_eq!(positions_after.len(), 1);

    // Verify that the position details haven't changed
    let position_after = market.query_position(long_position_id).unwrap();
    assert_eq!(initial_position, position_after);
}
