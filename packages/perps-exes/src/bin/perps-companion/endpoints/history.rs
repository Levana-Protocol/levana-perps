use crate::app::App;
use crate::endpoints::{ErrorPage, ExportHistory};
use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use cosmos::{Address, Contract};
use csv::WriterBuilder;
use msg::contracts::market::entry::{PositionAction, StatusResp, TraderActionHistoryResp};
use msg::contracts::market::position::PositionId;
use msg::prelude::{
    Collateral, DirectionToBase, MarketId, MarketQueryMsg, OrderInMessage, RawAddr, Signed,
};
use reqwest::{
    header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    StatusCode,
};
use std::fmt::Debug;
use std::sync::Arc;

const FILENAME: &str = "levana-history.csv";

pub(crate) async fn history(
    ExportHistory {
        chain,
        market,
        wallet,
    }: ExportHistory,
    app: State<Arc<App>>,
) -> impl IntoResponse {
    let history = History::new(chain, market, wallet);
    history
        .get_history_actions(&app)
        .await
        .map(HistoryActions::csv)
}

#[derive(serde::Deserialize, Debug)]
struct History {
    chain: String,
    market: Address,
    wallet: Address,
}

impl History {
    fn new(chain: String, market: Address, wallet: Address) -> History {
        History {
            chain,
            market,
            wallet,
        }
    }

    async fn get_history_actions(self, app: &App) -> Result<HistoryActions, Error> {
        let market_status = self.query_market_stats(app).await?;
        let position_actions = self.query_position_actions(app).await?;

        Ok(HistoryActions {
            market_status,
            position_actions,
        })
    }

    async fn query_position_actions(&self, app: &App) -> Result<Vec<PositionAction>, Error> {
        let cosmos = app.cosmos.get(&self.chain).ok_or(Error::UnknownChainId)?;
        let contract = PerpsContract(cosmos.make_contract(self.market));
        let mut actions = Vec::<PositionAction>::new();
        let mut start_after = None::<String>;

        loop {
            let mut res = contract
                .query::<MarketQueryMsg, TraderActionHistoryResp>(
                    MarketQueryMsg::TraderActionHistory {
                        owner: RawAddr::from(self.wallet.to_string()),
                        start_after: start_after.clone(),
                        limit: None, //FIXME fill in limit
                        order: Some(OrderInMessage::Ascending),
                    },
                    QueryType::TraderActionHistory,
                )
                .await?;

            actions.append(&mut res.actions);

            match res.next_start_after {
                None => break,
                Some(next_start_after) => start_after = Some(next_start_after),
            }
        }

        Ok(actions)
    }

    async fn query_market_stats(&self, app: &App) -> Result<StatusResp, Error> {
        let cosmos = app.cosmos.get(&self.chain).ok_or(Error::UnknownChainId)?;
        let contract = PerpsContract(cosmos.make_contract(self.market));
        let res = contract
            .query::<MarketQueryMsg, StatusResp>(
                MarketQueryMsg::Status { price: None },
                QueryType::Status,
            )
            .await?;

        Ok(res)
    }
}

struct HistoryActions {
    market_status: StatusResp,
    position_actions: Vec<PositionAction>,
}

impl HistoryActions {
    fn csv(self) -> Response {
        match self.csv_inner() {
            Ok(res) => res,
            Err(err) => {
                let mut res = format!("Error while generating CSV: {err:?}").into_response();
                *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                res
            }
        }
    }

    fn csv_inner(&self) -> Result<Response> {
        let mut writer = WriterBuilder::new().has_headers(false).from_writer(vec![]);

        writer.write_record([
            "Transaction Time (UTC)",
            "Position ID",
            "Market",
            "Direction",
            "Action",
            "Collateral Asset",
            "Amount",
        ])?;

        for position_action in &self.position_actions {
            let record = ActionRecord::from_position_action(&position_action, &self.market_status)?;
            writer.serialize(record)?;
        }

        let inner = writer.into_inner()?;
        let mut res = String::from_utf8(inner)?.into_response();
        let disposition = format!("attachment; filename={FILENAME}");

        res.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static(mime::TEXT_CSV.as_ref()),
        );
        res.headers_mut().insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition).unwrap(),
        );

        Ok(res)
    }
}

#[derive(Debug, serde::Serialize)]
struct ActionRecord {
    transaction_time: String,
    position_id: PositionId,
    market: MarketId,
    direction: String,
    kind: String,
    collateral_asset: String,
    amount: Signed<Collateral>,
}

impl ActionRecord {
    fn from_position_action(action: &PositionAction, status: &StatusResp) -> Result<Self> {
        let dt = action.timestamp.try_into_chrono_datetime()?;
        let transaction_time = dt.format("%Y-%m-%d %H:%M:%S").to_string();
        let position_id = action.id.context("Position action did not include id")?;
        // FIXME replace _ with a -
        let market = status.market_id.clone();
        let direction = match action.direction {
            DirectionToBase::Long => "Long",
            DirectionToBase::Short => "Short",
        }
        .to_string();
        let kind = action.kind.to_string();
        let collateral_asset = status.market_id.get_collateral().to_string();
        let amount = action.transfer_collateral;

        Ok(ActionRecord {
            transaction_time,
            position_id,
            market,
            direction,
            kind,
            collateral_asset,
            amount,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum QueryType {
    TraderActionHistory,
    Status,
}

struct PerpsContract(Contract);

impl PerpsContract {
    async fn query<M, T>(&self, msg: M, query_type: QueryType) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned,
        M: serde::Serialize + Debug,
    {
        let mut attempt = 1;
        loop {
            let res = self.0.query(&msg).await.map_err(|source| {
                //FIXME can this be abstracted?
                let e = Error::FailedToQueryContract {
                    msg: format!("{:?}", msg),
                    query_type,
                };
                log::error!("Attempt #{attempt}: {e}. {source:?}");
                e
            });

            match res {
                Ok(x) => break Ok(x),
                Err(e) => {
                    if attempt >= 5 {
                        break Err(e);
                    } else {
                        attempt += 1;
                    }
                }
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Unknown chain ID")]
    UnknownChainId,
    #[error("Failed to query contract with {query_type:?}\nQuery: {msg:?}")]
    FailedToQueryContract { msg: String, query_type: QueryType },
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        ErrorPage {
            code: match &self {
                Error::UnknownChainId => StatusCode::BAD_REQUEST,
                Error::FailedToQueryContract { query_type, msg: _ } => match query_type {
                    QueryType::Status => StatusCode::BAD_REQUEST,
                    QueryType::TraderActionHistory => StatusCode::INTERNAL_SERVER_ERROR,
                },
            },
            error: self,
        }
        .into_response()
    }
}
