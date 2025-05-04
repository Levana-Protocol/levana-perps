use cosmwasm_std::{
    testing::MockQuerier, to_json_binary, Addr, Binary, Decimal256, Deps, DepsMut, Env, GrpcQuery,
    MessageInfo, Querier, QueryRequest, Response, StdError, StdResult, SystemResult,
};
use cw_multi_test::{App, AppBuilder, AppResponse, ContractWrapper, Executor};
use perpswap::{
    contracts::market::{
        entry::{InstantiateMsg, QueryMsg},
        spot_price::{SpotPriceConfigInit, SpotPriceFeedDataInit, SpotPriceFeedInit},
    },
    prelude::*,
    token::TokenInit,
};
use prost::Message;
use rujira_rs::proto::types::{QueryPoolRequest, QueryPoolResponse};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::str::FromStr;

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

#[cw_serde]
pub enum OracleExecuteMsg {
    SetPrice {
        price: Decimal256,
        timestamp: Option<Uint64>,
    },
}

#[cw_serde]
pub struct MockOraclePriceFeedRujiraResp {
    pub price: Decimal256,
    pub volatile: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MockOraclePriceResp {
    #[serde(default)]
    pub rujira: BTreeMap<String, MockOraclePriceFeedRujiraResp>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum MockPriceResponse {
    Valid,
    Zero,
    NaN,
}

pub fn setup_test_env(price_response: MockPriceResponse) -> (App, Addr) {
    let mut app = AppBuilder::new().build(|_, _, _| {});
    let contract_addr = setup_mock_market(&mut app, price_response).unwrap();
    (app, contract_addr)
}

pub fn exec_set_oracle_price_base(
    app: &mut App,
    contract_addr: &Addr,
    price: Decimal256,
    timestamp: Option<Uint64>,
) -> Result<AppResponse> {
    let value = app.execute_contract(
        Addr::unchecked("admin"),
        contract_addr.clone(),
        &OracleExecuteMsg::SetPrice { price, timestamp },
        &[],
    )?;
    Ok(value)
}

pub fn exec_set_oracle_price_usd(
    app: &mut App,
    contract_addr: &Addr,
    price: Decimal256,
    timestamp: Option<Uint64>,
) -> Result<AppResponse> {
    let value = app.execute_contract(
        Addr::unchecked("admin"),
        contract_addr.clone(),
        &OracleExecuteMsg::SetPrice { price, timestamp },
        &[],
    )?;
    Ok(value)
}

pub fn setup_mock_market(
    app: &mut App,
    price_response: MockPriceResponse,
) -> Result<Addr, StdError> {
    fn query_valid(_deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary> {
        match msg {
            QueryMsg::OraclePrice { .. } => {
                let price = Decimal256::from_str("10").unwrap();
                let mut rujira = BTreeMap::new();
                rujira.insert(
                    "ETH.RUNE".to_string(),
                    MockOraclePriceFeedRujiraResp {
                        price,
                        volatile: false,
                    },
                );

                let resp = MockOraclePriceResp { rujira };
                Ok(to_json_binary(&resp).unwrap())
            }
            _ => Err(StdError::generic_err("Unsupported query").into()),
        }
    }

    fn query_zero(_deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary> {
        match msg {
            QueryMsg::OraclePrice { .. } => {
                let price = Decimal256::from_str("10").unwrap();
                let mut rujira = BTreeMap::new();
                rujira.insert(
                    "ETH.RUNE".to_string(),
                    MockOraclePriceFeedRujiraResp {
                        price,
                        volatile: true,
                    },
                );

                let resp = MockOraclePriceResp { rujira };
                let original = serde_json::to_string(&resp).unwrap();
                let mut val: Value = serde_json::from_str(&original).unwrap();
                val["rujira"]["ETH.RUNE"]["price"] = json!("0");
                let modified = val.to_string();
                Ok(Binary::from(modified.into_bytes()))
            }
            _ => Err(StdError::generic_err("Unsupported query").into()),
        }
    }

    fn query_nan(_deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary> {
        match msg {
            QueryMsg::OraclePrice { .. } => {
                let price = Decimal256::from_str("10").unwrap();
                let mut rujira = BTreeMap::new();
                rujira.insert(
                    "ETH.RUNE".to_string(),
                    MockOraclePriceFeedRujiraResp {
                        price,
                        volatile: true,
                    },
                );

                let resp = MockOraclePriceResp { rujira };
                let original = serde_json::to_string(&resp).unwrap();
                let mut val: Value = serde_json::from_str(&original).unwrap();
                val["rujira"]["ETH.RUNE"]["price"] = json!("NaN");
                let modified = val.to_string();
                Ok(Binary::from(modified.into_bytes()))
            }
            _ => Err(StdError::generic_err("Unsupported query").into()),
        }
    }

    let query = match price_response {
        MockPriceResponse::Valid => query_valid,
        MockPriceResponse::Zero => query_zero,
        MockPriceResponse::NaN => query_nan,
    };

    fn execute(
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        msg: OracleExecuteMsg,
    ) -> StdResult<Response> {
        match msg {
            OracleExecuteMsg::SetPrice { price, timestamp } => {
                // Validar que el remitente sea el administrador
                let admin = deps
                    .storage
                    .get(b"admin")
                    .ok_or_else(|| StdError::generic_err("Admin not set"))?;
                if info.sender.as_bytes() != admin {
                    return Err(StdError::generic_err(
                        "Unauthorized: only admin can set price",
                    ));
                }

                // Guardar el precio
                deps.storage.set(b"price", &price.to_string().into_bytes());

                // Guardar el timestamp si se proporciona
                if let Some(ts) = timestamp {
                    deps.storage.set(b"timestamp", &ts.to_string().into_bytes());
                } else {
                    deps.storage.remove(b"timestamp");
                }

                Ok(Response::new()
                    .add_attribute("action", "set_price")
                    .add_attribute("price", price.to_string())
                    .add_attribute("timestamp", timestamp.unwrap_or(Uint64::zero()).to_string()))
            }
        }
    }

    fn instantiate(
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        _msg: InstantiateMsg,
    ) -> StdResult<Response> {
        deps.storage.set(b"admin", info.sender.as_bytes());
        Ok(Response::default())
    }

    let code = ContractWrapper::new(execute, instantiate, query);
    let code_id = app.store_code(Box::new(code));

    let init_msg = InstantiateMsg {
        factory: Addr::unchecked("factory").into(),
        config: None,
        spot_price: SpotPriceConfigInit::Oracle {
            pyth: None,
            stride: None,
            feeds: vec![SpotPriceFeedInit {
                inverted: false,
                volatile: None,
                data: SpotPriceFeedDataInit::Constant {
                    price: NonZero::try_from("100").unwrap(),
                },
            }
            .into()],
            feeds_usd: vec![],
            volatile_diff_seconds: None,
        },
        initial_price: None,
        market_id: MarketId::new("ETH", "RUNE", MarketType::CollateralIsQuote),
        token: TokenInit::Native {
            denom: "USD".to_string(),
            decimal_places: 6,
        },
        initial_borrow_fee_rate: Decimal256::zero(),
    };

    app.instantiate_contract(
        code_id,
        Addr::unchecked("admin"),
        &init_msg,
        &[],
        "MockMarket",
        None,
    )
    .map_err(|err| StdError::generic_err(err.to_string()))
}
