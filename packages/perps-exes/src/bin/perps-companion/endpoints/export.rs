use crate::app::App;
use crate::endpoints::{ErrorPage, ExportHistory};
use anyhow::Result;
use axum::extract::State;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use cosmos::{Address, Contract};
use csv::WriterBuilder;
use msg::contracts::market::entry::{
    LpAction, LpActionHistoryResp, LpActionKind, PositionAction, PositionActionKind, StatusResp,
    TraderActionHistoryResp,
};
use msg::prelude::{
    DirectionToBase, MarketQueryMsg, OrderInMessage, RawAddr, Signed
};
use perps_exes::prelude::{Collateral, UnsignedDecimal};
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use reqwest::StatusCode;
use serde::Serialize;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;

// Route Handlers

const FILENAME: &str = "levana-history.csv";
const TRADER_ACTION_HISTORY_LIMIT: Option<u32> = Some(20);
const LP_ACTION_HISTORY_LIMIT: Option<u32> = Some(20);

pub(crate) async fn history(
    ExportHistory {
        chain,
        market,
        wallet,
    }: ExportHistory,
    app: State<Arc<App>>,
) -> impl IntoResponse {
    let exporter = Exporter::new(&app, chain, market, wallet)?;

    exporter.export().await.map(|csv| {
        let mut res = csv.into_response();
        let disposition = format!("attachment; filename={FILENAME}");

        res.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static(mime::TEXT_CSV.as_ref()),
        );
        res.headers_mut().insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition).unwrap(),
        );

        res
    })
}

// Export Logic

struct Exporter {
    /// The wallet address of the user for whom to generate a report
    wallet: Address,
    /// The address of the market contract
    contract: PerpsContract,
}

impl Exporter {
    /// Returns a new Exporter
    ///
    /// * chain - The chain id of the desired chain.
    /// * market - The address of the market contract
    /// * wallet - The wallet address for which to export trader and LP history
    pub(crate) fn new(
        app: &App,
        chain: String,
        market: Address,
        wallet: Address,
    ) -> Result<Self, Error> {
        let cosmos = app.cosmos.get(&chain).ok_or(Error::UnknownChainId)?;
        let contract = PerpsContract(cosmos.make_contract(market));

        Ok(Exporter { wallet, contract })
    }

    /// Queries the specified market contract for trader and LP history and generates a CSV
    async fn export(&self) -> Result<String, Error> {
        let status = self.query_market_stats().await?;
        let position_actions = self.query_position_actions().await?;
        let lp_actions = self.query_lp_actions().await?;
        let csv_generator = CsvGenerator::new(status, position_actions, lp_actions);
        let records = csv_generator.get_action_records()?;
        let csv = csv_generator
            .generate_csv(records)
            .map_err(|_| Error::FailedToGenerateCsv)?;

        Ok(csv)
    }

    /// Query the position (aka trader) actions, paginating until complete
    async fn query_position_actions(&self) -> Result<Vec<PositionAction>, Error> {
        let mut actions = Vec::<PositionAction>::new();
        let mut start_after = None::<String>;

        loop {
            let mut res = self
                .contract
                .query::<MarketQueryMsg, TraderActionHistoryResp>(
                    MarketQueryMsg::TraderActionHistory {
                        owner: RawAddr::from(self.wallet.to_string()),
                        start_after: start_after.clone(),
                        limit: TRADER_ACTION_HISTORY_LIMIT,
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

    /// Query the LP actions, paginating until complete
    async fn query_lp_actions(&self) -> Result<Vec<LpAction>, Error> {
        let mut actions = Vec::<LpAction>::new();
        let mut start_after = None::<String>;

        loop {
            let mut res = self
                .contract
                .query::<MarketQueryMsg, LpActionHistoryResp>(
                    MarketQueryMsg::LpActionHistory {
                        addr: RawAddr::from(self.wallet.to_string()),
                        start_after: start_after.clone(),
                        limit: LP_ACTION_HISTORY_LIMIT,
                        order: Some(OrderInMessage::Ascending),
                    },
                    QueryType::LpActionHistory,
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

    /// Get the market stats
    async fn query_market_stats(&self) -> Result<StatusResp, Error> {
        let res = self
            .contract
            .query::<MarketQueryMsg, StatusResp>(
                MarketQueryMsg::Status { price: None },
                QueryType::Status,
            )
            .await?;

        Ok(res)
    }
}

struct CsvGenerator {
    status: StatusResp,
    position_actions: Vec<PositionAction>,
    lp_actions: Vec<LpAction>
}

impl CsvGenerator {
    /// Returns a new CsvGenerator
    fn new(status: StatusResp, position_actions: Vec<PositionAction>, lp_actions: Vec<LpAction>) -> CsvGenerator {
        CsvGenerator {
            status,
            position_actions,
            lp_actions
        }
    }

    /// Create a vec of [ActionRecord]s by zipping [PositionAction]s and [LpAction]s together, sorted
    /// chronologically, and filtering out actions that didn't actually move and collateral (e.g. update leverage)
    fn get_action_records(
        &self,
    ) -> Result<Vec<ActionRecord>, Error> {
        let mut position_actions_iter = self.position_actions
            .iter()
            .filter(|action| !action.transfer_collateral.is_zero());
        let mut lp_actions_iter = self.lp_actions
            .iter()
            .filter(|action| !action.collateral.is_zero());
        let mut next_position_action = position_actions_iter.next();
        let mut next_lp_action = lp_actions_iter.next();
        let mut records = Vec::<ActionRecord>::new();

        loop {
            let record = match (next_position_action, next_lp_action) {
                (Some(position_action), Some(lp_action)) => {
                    if position_action.timestamp <= lp_action.timestamp {
                        next_position_action = position_actions_iter.next();
                        ActionRecord::from_position_action(position_action, &self.status)?
                    } else {
                        next_lp_action = lp_actions_iter.next();
                        ActionRecord::from_lp_action(lp_action, &self.status)?
                    }
                }
                (Some(position_action), None) => {
                    next_position_action = position_actions_iter.next();
                    ActionRecord::from_position_action(position_action, &self.status)?
                }
                (None, Some(lp_action)) => {
                    next_lp_action = lp_actions_iter.next();
                    ActionRecord::from_lp_action(lp_action, &self.status)?
                }
                (None, None) => break,
            };

            records.push(record);
        }

        Ok(records)
    }

    /// Creates a CSV from the provided records. The CSV headers are hardcoded.
    pub(crate) fn generate_csv(&self, actions: Vec<ActionRecord>) -> Result<String> {
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

        for action in actions {
            writer.serialize(action)?;
        }

        let inner = writer.into_inner()?;
        let data = String::from_utf8(inner)?;

        Ok(data)
    }
}

#[derive(Debug, Serialize)]
struct ActionRecord {
    transaction_time: String,
    position_id: String,
    market_id: String,
    direction: String,
    kind: String,
    collateral_asset: String,
    amount: Signed<Collateral>,
}

impl ActionRecord {
    /// Converts a PositionAction into an ActionRecord
    fn from_position_action(action: &PositionAction, status: &StatusResp) -> Result<Self, Error> {
        let dt = action
            .timestamp
            .try_into_chrono_datetime()
            .map_err(|_| Error::FailedToGenerateCsv)?;
        let transaction_time = dt.format("%Y-%m-%d %H:%M:%S").to_string();
        let position_id = action.id.ok_or(Error::FailedToGenerateCsv)?.to_string();
        let market_id = status.market_id.clone().to_string().replace("_", "-");
        let direction = match action.direction {
            DirectionToBase::Long => "Long",
            DirectionToBase::Short => "Short",
        }
        .to_string();
        let kind = ActionRecordKind::Position(action.kind.clone()).to_string();
        let collateral_asset = status.market_id.get_collateral().to_string();
        let amount = action.transfer_collateral;

        Ok(ActionRecord {
            transaction_time,
            position_id,
            market_id,
            direction,
            kind,
            collateral_asset,
            amount,
        })
    }

    /// Converts a LpAction into an ActionRecord
    fn from_lp_action(action: &LpAction, status: &StatusResp) -> Result<Self, Error> {
        let dt = action
            .timestamp
            .try_into_chrono_datetime()
            .map_err(|_| Error::FailedToGenerateCsv)?;
        let transaction_time = dt.format("%Y-%m-%d %H:%M:%S").to_string();
        let position_id = "-".to_string();
        let market_id = status.market_id.clone().to_string().replace("_", "-");
        let direction = "-".to_string();
        let kind = ActionRecordKind::Lp(action.kind.clone()).to_string();
        let collateral_asset = status.market_id.get_collateral().to_string();
        let amount = action.collateral.into_signed();

        Ok(ActionRecord {
            transaction_time,
            position_id,
            market_id,
            direction,
            kind,
            collateral_asset,
            amount,
        })
    }
}

#[derive(Debug, Serialize)]
enum ActionRecordKind {
    Position(PositionActionKind),
    Lp(LpActionKind),
}

impl Display for ActionRecordKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            ActionRecordKind::Position(action) => match action {
                PositionActionKind::Open => "Open",
                PositionActionKind::Update => "Update",
                PositionActionKind::Close => "Close",
                PositionActionKind::Transfer => "Transfer",
            },
            ActionRecordKind::Lp(action) => match action {
                LpActionKind::DepositLp => "Deposit LP",
                LpActionKind::DepositXlp => "Deposit xLP",
                LpActionKind::ReinvestYieldLp => "Reinvest Yield LP",
                LpActionKind::ReinvestYieldXlp => "Reinvest Yield xLP",
                LpActionKind::UnstakeXlp => "Convert xLP-LP",
                LpActionKind::CollectLp => "Collect LP",
                LpActionKind::Withdraw => "Withdraw LP",
                LpActionKind::ClaimYield => "Claim Yield",
            },
        };

        f.write_str(&str)
    }
}

// Query & Response

#[derive(Clone, Copy, Debug)]
pub(crate) enum QueryType {
    TraderActionHistory,
    LpActionHistory,
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
    #[error("Failed to generate CSV")]
    FailedToGenerateCsv,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        ErrorPage {
            code: match &self {
                Error::UnknownChainId => StatusCode::BAD_REQUEST,
                Error::FailedToQueryContract { query_type, msg: _ } => match query_type {
                    QueryType::Status => StatusCode::BAD_REQUEST,
                    QueryType::TraderActionHistory => StatusCode::INTERNAL_SERVER_ERROR,
                    QueryType::LpActionHistory => StatusCode::INTERNAL_SERVER_ERROR,
                },
                Error::FailedToGenerateCsv => StatusCode::INTERNAL_SERVER_ERROR,
            },
            error: self,
        }
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoints::export::Exporter;

    #[test]
    fn test_export_history() {
        assert!(true)
    }
}
