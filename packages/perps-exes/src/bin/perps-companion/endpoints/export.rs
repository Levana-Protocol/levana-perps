use crate::app::App;
use crate::endpoints::ExportHistory;
use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use cosmos::{Address, Contract};
use csv::WriterBuilder;
use msg::contracts::market::entry::{PositionAction, StatusResp, TraderActionHistoryResp};
use msg::contracts::market::position::PositionId;
use msg::prelude::{DirectionToBase, MarketId, MarketQueryMsg, OrderInMessage, RawAddr, Signed};
use perps_exes::prelude::Collateral;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use serde::Serialize;
use std::fmt::Debug;
use std::sync::Arc;

//todo add logging?

// Route Handlers

const FILENAME: &str = "levana-trade-history.csv";

pub(crate) async fn history(
    ExportHistory {
        chain,
        market,
        wallet,
    }: ExportHistory,
    app: State<Arc<App>>,
) -> Response {
    //todo we shouldn't need unwrap here, look at PnL as an example
    let exporter = Exporter::new(&app, chain, market, wallet).unwrap();

    //todo should I make a CSV Response (like https://docs.rs/axum/latest/axum/struct.Json.html)
    let csv = exporter.export().await.unwrap();
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
}

// Export Logic

struct Exporter {
    wallet: Address,
    contract: PerpsContract,
}

impl Exporter {
    pub(crate) fn new(app: &App, chain: String, market: Address, wallet: Address) -> Result<Self> {
        let cosmos = app.cosmos.get(&chain).ok_or(Error::UnknownChainId)?;
        let contract = PerpsContract(cosmos.make_contract(market));

        Ok(Exporter { wallet, contract })
    }
}

#[derive(Debug, Serialize)]
struct TradeAction {
    transaction_time: String,
    position_id: PositionId,
    market: MarketId,
    direction: String,
    action: String,
    collateral_asset: String,
    amount: Signed<Collateral>,
}

impl TradeAction {
    fn from_position_action(position_action: &PositionAction, status: &StatusResp) -> Result<Self> {
        let dt = position_action.timestamp.try_into_chrono_datetime()?;
        let transaction_time = dt.format("%Y-%m-%d %H:%M:%S").to_string();
        let position_id = position_action
            .id
            .context("Position action did not include id")?;
        let market = status.market_id.clone();
        let direction = match position_action.direction {
            DirectionToBase::Long => "Long",
            DirectionToBase::Short => "Short",
        }
        .to_string();
        let action = position_action.kind.to_string();
        let collateral_asset = status.market_id.get_collateral().to_string();
        let amount = position_action.transfer_collateral;

        Ok(TradeAction {
            transaction_time,
            position_id,
            market,
            direction,
            action,
            collateral_asset,
            amount,
        })
    }
}

impl Exporter {
    async fn export(&self) -> Result<String> {
        let status = self.query_market_stats().await?;
        let position_actions = self.query_position_actions().await?;
        let trade_actions = position_actions
            .iter()
            .map(|position_action| TradeAction::from_position_action(position_action, &status))
            .collect::<Result<Vec<TradeAction>, _>>()?;
        let csv = self.generate_csv(trade_actions)?;

        Ok(csv)
    }

    async fn query_position_actions(&self) -> Result<Vec<PositionAction>> {
        let mut actions = Vec::<PositionAction>::new();
        let mut start_after = None::<String>;

        loop {
            let mut res = self
                .contract
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

    async fn query_market_stats(&self) -> Result<StatusResp> {
        let res = self
            .contract
            .query::<MarketQueryMsg, StatusResp>(
                MarketQueryMsg::Status { price: None },
                QueryType::Status,
            )
            .await?;

        Ok(res)
    }

    pub(crate) fn generate_csv(&self, actions: Vec<TradeAction>) -> Result<String> {
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

// Query & Response

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
        todo!()
    }
}
