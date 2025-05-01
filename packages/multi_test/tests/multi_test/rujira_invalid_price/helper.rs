use cosmwasm_std::{
    testing::MockQuerier, Addr, Binary, GrpcQuery, Querier, QueryRequest, SystemResult,
};
use cw_multi_test::{App, AppBuilder};
use cw_storage_plus::Item;
use perpswap::namespace;
use prost::Message;
use rujira_rs::proto::types::{QueryPoolRequest, QueryPoolResponse};

pub const FACTORY_ADDR: Item<Addr> = Item::new(namespace::FACTORY_ADDR);

pub const _GOVERNANCE: &str = "cosmwasm1h72z9g4qf2kjrq866zgn78xl32wn0q8aqayp05jkjpgdp2qft5aquanhrh";
pub const MARKET_ADDR1: &str =
    "cosmwasm1qnufjmd8vwm6j6d3q28wxqr4d8408f34fpka4vs365fvskualrasv5ues5";
pub const _MARKET_ADDR2: &str =
    "cosmwasm1vqjarrly327529599rcc4qhzvhwe34pp5uyy4gylvxe5zupeqx3sg08lap";
pub struct CustomGrpcQuerier {
    pub base: MockQuerier,
}

impl Querier for CustomGrpcQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> cosmwasm_std::QuerierResult {
        let request: QueryRequest = match cosmwasm_std::from_json(bin_request) {
            Ok(req) => req,
            Err(e) => {
                return SystemResult::Err(cosmwasm_std::SystemError::InvalidRequest {
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
                        asset_tor_price: "0".to_owned(),
                        status: "Available".to_owned(),
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
                        return SystemResult::Err(cosmwasm_std::SystemError::InvalidResponse {
                            error: "Response encode error".to_owned(),
                            response: Binary::new(vec![]),
                        });
                    }

                    SystemResult::Ok(cosmwasm_std::ContractResult::Ok(Binary::from(buf)))
                } else {
                    SystemResult::Err(cosmwasm_std::SystemError::UnsupportedRequest {
                        kind: "Unknown grpc path".to_owned(),
                    })
                }
            }
            _ => self.base.raw_query(bin_request),
        }
    }
}

pub async fn setup_test_env(price: Option<&str>) -> (App, Addr) {
    let asset = "ETH.RUNE".to_owned();
    let req = QueryPoolRequest {
        asset: asset.clone(),
        height: "0".to_string(),
    };

    let mut buf = Vec::new();
    req.encode(&mut buf)
        .expect("QueryPoolRequest encoding failed.");
    let path = "/types.Query/Pool".to_owned();

    let price = price.unwrap_or("NaN");

    let res = QueryPoolResponse {
        asset,
        asset_tor_price: price.to_owned(),
        status: "Available".to_owned(),
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

    server
        .start()
        .await
        .expect("Rujira GRPC Mock server should start");

    let app = AppBuilder::new().build(|_, _, _| {});

    // We need store and instantiate the market contract

    // here is a general example
    // https://github.com/Levana-Protocol/levana-perps/blob/main/packages/multi_test/tests/multi_test/vault/helpers.rs

    // Market SpotPriceConfigInit

    // Instantiate market contract

    // Some contract_addr
    let contract_addr = Addr::unchecked(MARKET_ADDR1);

    (app, contract_addr, server)
}
