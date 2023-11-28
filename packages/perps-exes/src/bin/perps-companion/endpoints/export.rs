use crate::app::App;
use crate::endpoints::{ErrorPage, ExportHistory};
use crate::types::ChainId;
use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use cosmos::{Address, Contract, Cosmos};
use cosmwasm_std::Addr;
use csv::WriterBuilder;
use itertools::{EitherOrBoth, Itertools};
use msg::contracts::factory::entry::{MarketInfoResponse, MarketsResp};
use msg::contracts::market::entry::{
    LpAction, LpActionHistoryResp, LpActionKind, PositionAction, PositionActionKind, StatusResp,
    TraderActionHistoryResp,
};
use msg::contracts::market::position::{PositionId, PositionsResp};
use msg::prelude::{
    DirectionToBase, FactoryQueryMsg, MarketQueryMsg, OrderInMessage, RawAddr, Signed,
};
use perps_exes::prelude::{Collateral, UnsignedDecimal};


use serde::Serialize;
use shared::storage::MarketId;
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;
use std::sync::Arc;

// Route Handlers

const TRADER_ACTION_HISTORY_LIMIT: Option<u32> = Some(20);
const LP_ACTION_HISTORY_LIMIT: Option<u32> = Some(20);
const POSITIONS_QUERY_CHUNK_SIZE: usize = 3;

pub(crate) async fn history(
    ExportHistory {
        chain,
        factory,
        wallet,
    }: ExportHistory,
    app: State<Arc<App>>,
) -> impl IntoResponse {
    let exporter = Exporter::new(&app, chain, factory, wallet)?;

    exporter.export().await.map(|csv| {
        let mut res = csv.into_response();
        let disposition = format!("attachment; filename=levana-history-{wallet}.csv");

        res.headers_mut().insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static(mime::TEXT_CSV.as_ref()),
        );
        res.headers_mut().insert(
            http::header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition).unwrap(),
        );

        res
    })
}

// Export Logic

fn get_action_records(
    status: StatusResp,
    position_actions: &[PositionAction],
    directions: HashMap<PositionId, DirectionToBase>,
    lp_actions: &[LpAction],
    wallet: &RawAddr,
) -> Result<Vec<ActionRecord>> {
    let mut action_records = Vec::<ActionRecord>::new();
    let merged_actions = position_actions
        .iter()
        .merge_join_by(lp_actions, |a, b| a.timestamp.cmp(&b.timestamp));
    let push_position_action =
        |records: &mut Vec<ActionRecord>, action: &PositionAction| -> Result<()> {
            if !action.transfer_collateral.is_zero() {
                let record = ActionRecord::from_position_action(
                    action,
                    &status,
                    directions[&action.id.context("position_action missing id")?],
                    wallet,
                )?;

                records.push(record);
            }

            Ok(())
        };
    let push_lp_action = |records: &mut Vec<ActionRecord>, action: &LpAction| -> Result<()> {
        if !action.collateral.is_zero() && action.kind != LpActionKind::UnstakeXlp {
            let record = ActionRecord::from_lp_action(action, &status)?;
            records.push(record)
        }

        Ok(())
    };

    for either_or_both in merged_actions {
        match either_or_both {
            EitherOrBoth::Left(position_action) => {
                push_position_action(&mut action_records, position_action)?;
            }
            EitherOrBoth::Right(lp_action) => {
                push_lp_action(&mut action_records, lp_action)?;
            }
            EitherOrBoth::Both(position_action, lp_action) => {
                push_position_action(&mut action_records, position_action)?;
                push_lp_action(&mut action_records, lp_action)?;
            }
        }
    }

    Ok(action_records)
}

fn generate_csv(action_records: &[ActionRecord]) -> Result<String> {
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(vec![]);

    writer.write_record([
        "Transaction Time (UTC)",
        "Position ID",
        "Market",
        "Direction",
        "Action",
        "Asset",
        "Amount",
    ])?;

    for record in action_records {
        writer.serialize(record)?;
    }

    let inner = writer.into_inner()?;
    let data = String::from_utf8(inner)?;

    Ok(data)
}

struct Exporter {
    /// The interface to the relevant cosmos chain
    cosmos: Cosmos,
    /// The wallet address of the user for whom to generate a report
    wallet: RawAddr,
    /// The address of the factory contract
    factory_contract: PerpsContract,
}

impl<'a> Exporter {
    /// Returns a new Exporter
    ///
    /// * chain_id - The chain id of the desired chain.
    /// * market - The address of the market contract
    /// * wallet - The wallet address for which to export trader and LP history
    pub(crate) fn new(
        app: &'a App,
        chain_id: ChainId,
        factory: Address,
        wallet: Address,
    ) -> Result<Self, Error> {
        let cosmos = app.cosmos.get(&chain_id).ok_or(Error::UnknownChainId)?;
        let factory_contract = PerpsContract(cosmos.make_contract(factory));
        let wallet = RawAddr::from(wallet.to_string());

        Ok(Exporter {
            cosmos: cosmos.clone(),
            wallet,
            factory_contract,
        })
    }

    /// Queries the specified market contract for trader and LP history and generates a CSV
    async fn export(&self) -> Result<String, Error> {
        let market_ids = self.query_market_ids().await?;
        let mut records = Vec::<ActionRecord>::new();

        for market_id in market_ids {
            let addr_str = self.query_market_addr(market_id).await?;
            let address = Address::from_str(addr_str.as_str()).map_err(|_| Error::Generic {
                msg: "unable to convert addr".to_string(),
            })?;
            let market_contract = PerpsContract(self.cosmos.make_contract(address));

            let status = self.query_market_stats(&market_contract).await?;
            let position_actions = self.query_position_actions(&market_contract).await?;
            let position_ids = position_actions
                .iter()
                .filter_map(|a| a.id)
                .unique()
                .collect::<Vec<PositionId>>();
            let directions = self
                .query_positions_direction(&market_contract, &position_ids)
                .await?;
            let lp_actions = self.query_lp_actions(&market_contract).await?;
            let mut new_records = get_action_records(
                status,
                &position_actions,
                directions,
                &lp_actions,
                &self.wallet,
            )
            .map_err(|_| Error::Generic {
                msg: "unable to create action records".to_string(),
            })?;

            records.append(&mut new_records);
        }

        let csv = generate_csv(&records).map_err(|_| Error::FailedToGenerateCsv)?;

        Ok(csv)
    }

    async fn query_market_ids(&self) -> Result<Vec<MarketId>, Error> {
        let mut start_after = None::<MarketId>;
        let mut markets = Vec::<MarketId>::new();

        loop {
            let mut res = self
                .factory_contract
                .query::<FactoryQueryMsg, MarketsResp>(
                    FactoryQueryMsg::Markets {
                        start_after,
                        limit: None,
                    },
                    QueryType::Markets,
                )
                .await?;

            match res.markets.last() {
                None => break,
                Some(last) => {
                    start_after = Some(last.clone());
                    markets.append(&mut res.markets);
                }
            }
        }

        Ok(markets)
    }

    async fn query_market_addr(&self, market_id: MarketId) -> Result<Addr, Error> {
        let res = self
            .factory_contract
            .query::<FactoryQueryMsg, MarketInfoResponse>(
                FactoryQueryMsg::MarketInfo { market_id },
                QueryType::MarketInfo,
            )
            .await?;

        Ok(res.market_addr)
    }

    /// Query the position (aka trader) actions, paginating until complete
    async fn query_position_actions(
        &self,
        contract: &PerpsContract,
    ) -> Result<Vec<PositionAction>, Error> {
        let mut actions = Vec::<PositionAction>::new();
        let mut start_after = None::<String>;

        loop {
            let mut res = contract
                .query::<MarketQueryMsg, TraderActionHistoryResp>(
                    MarketQueryMsg::TraderActionHistory {
                        owner: self.wallet.clone(),
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

    /// Query the position (aka trader) actions, paginating until complete
    async fn query_positions_direction(
        &self,
        contract: &PerpsContract,
        position_ids: &[PositionId],
    ) -> Result<HashMap<PositionId, DirectionToBase>, Error> {
        let mut directions = HashMap::<PositionId, DirectionToBase>::new();
        let mut chunks = position_ids.chunks(POSITIONS_QUERY_CHUNK_SIZE);

        loop {
            match chunks.next() {
                None => break,
                Some(position_ids) => {
                    let res = contract
                        .query::<MarketQueryMsg, PositionsResp>(
                            MarketQueryMsg::Positions {
                                position_ids: position_ids.to_vec(),
                                skip_calc_pending_fees: None,
                                fees: None,
                                price: None,
                            },
                            QueryType::Positions,
                        )
                        .await?;

                    for pos in res.positions {
                        directions.insert(pos.id, pos.direction_to_base);
                    }

                    for pos in res.pending_close {
                        directions.insert(pos.id, pos.direction_to_base);
                    }

                    for pos in res.closed {
                        directions.insert(pos.id, pos.direction_to_base);
                    }
                }
            }
        }

        Ok(directions)
    }

    /// Query the LP actions, paginating until complete
    async fn query_lp_actions(&self, contract: &PerpsContract) -> Result<Vec<LpAction>, Error> {
        let mut actions = Vec::<LpAction>::new();
        let mut start_after = None::<String>;

        loop {
            let mut res = contract
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
    async fn query_market_stats(&self, contract: &PerpsContract) -> Result<StatusResp, Error> {
        let res = contract
            .query::<MarketQueryMsg, StatusResp>(
                MarketQueryMsg::Status { price: None },
                QueryType::Status,
            )
            .await?;

        Ok(res)
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
    fn from_position_action(
        action: &PositionAction,
        status: &StatusResp,
        direction: DirectionToBase,
        wallet: &RawAddr,
    ) -> Result<Self, Error> {
        let dt = action
            .timestamp
            .try_into_chrono_datetime()
            .map_err(|_| Error::FailedToGenerateCsv)?;
        let transaction_time = dt.format("%Y-%m-%d %H:%M:%S").to_string();
        let position_id = action.id.ok_or(Error::FailedToGenerateCsv)?.to_string();
        let market_id = status.market_id.clone().to_string().replace('_', "-");
        let direction = match direction {
            DirectionToBase::Long => "Long",
            DirectionToBase::Short => "Short",
        }
        .to_string();
        let kind = match action.clone().kind {
            PositionActionKind::Transfer => {
                let new_owner: RawAddr = action
                    .new_owner
                    .as_ref()
                    .ok_or(Error::FailedToGenerateCsv)?
                    .into();
                if new_owner == *wallet {
                    "Received Position".to_owned()
                } else {
                    "Sent Position".to_owned()
                }
            }
            kind => ActionRecordKind::Position(kind).to_string(),
        };

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

    /// Converts an LpAction into an ActionRecord
    fn from_lp_action(action: &LpAction, status: &StatusResp) -> Result<Self, Error> {
        let dt = action
            .timestamp
            .try_into_chrono_datetime()
            .map_err(|_| Error::FailedToGenerateCsv)?;
        let transaction_time = dt.format("%Y-%m-%d %H:%M:%S").to_string();
        let position_id = "-".to_string();
        let market_id = status.market_id.clone().to_string().replace('_', "-");
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
                LpActionKind::UnstakeXlp => "Unstake xLP",
                LpActionKind::CollectLp => "Convert xLP-LP",
                LpActionKind::Withdraw => "Withdraw LP",
                LpActionKind::ClaimYield => "Claim Yield",
            },
        };

        f.write_str(str)
    }
}

// Query & Response

#[derive(Clone, Copy, Debug)]
pub(crate) enum QueryType {
    TraderActionHistory,
    LpActionHistory,
    Status,
    Positions,
    Markets,
    MarketInfo,
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
    #[error("CSV export error: {msg:?}")]
    Generic { msg: String },
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        ErrorPage {
            code: match &self {
                Error::UnknownChainId => http::status::StatusCode::BAD_REQUEST,
                Error::FailedToQueryContract { query_type, msg: _ } => match query_type {
                    QueryType::Status => http::status::StatusCode::BAD_REQUEST,
                    QueryType::TraderActionHistory => http::status::StatusCode::INTERNAL_SERVER_ERROR,
                    QueryType::LpActionHistory => http::status::StatusCode::INTERNAL_SERVER_ERROR,
                    QueryType::Positions => http::status::StatusCode::INTERNAL_SERVER_ERROR,
                    QueryType::Markets => http::status::StatusCode::BAD_REQUEST,
                    QueryType::MarketInfo => http::status::StatusCode::INTERNAL_SERVER_ERROR,
                },
                Error::FailedToGenerateCsv => http::status::StatusCode::INTERNAL_SERVER_ERROR,
                Error::Generic { .. } => http::status::StatusCode::INTERNAL_SERVER_ERROR,
            },
            error: self,
        }
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use crate::endpoints::export::{generate_csv, get_action_records};
    use cosmwasm_std::Addr;
    use msg::contracts::market::config::Config;
    use msg::contracts::market::entry::{
        Fees, LpAction, LpActionKind, PositionAction, PositionActionKind, StatusResp,
    };
    use msg::contracts::market::position::PositionId;
    use msg::contracts::market::spot_price::SpotPriceConfig;
    use msg::token::Token;
    use shared::market_type::MarketType;
    use shared::number::{Collateral, Signed};
    use shared::prelude::{DirectionToBase, Timestamp};
    use shared::storage::MarketId;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn new_status_resp() -> StatusResp {
        StatusResp {
            market_id: MarketId::new("BASE", "QUOTE", MarketType::CollateralIsBase),
            base: "".to_string(),
            quote: "".to_string(),
            market_type: MarketType::CollateralIsQuote,
            collateral: Token::Native {
                denom: "".to_string(),
                decimal_places: 0,
            },
            config: Config::new(SpotPriceConfig::Manual {
                admin: Addr::unchecked("manual-price-owner"),
            }),
            liquidity: Default::default(),
            next_crank: None,
            last_crank_completed: None,
            unpend_queue_size: 0,
            borrow_fee: Default::default(),
            borrow_fee_lp: Default::default(),
            borrow_fee_xlp: Default::default(),
            long_funding: Default::default(),
            short_funding: Default::default(),
            long_notional: Default::default(),
            short_notional: Default::default(),
            long_usd: Default::default(),
            short_usd: Default::default(),
            instant_delta_neutrality_fee_value: Default::default(),
            delta_neutrality_fee_fund: Default::default(),
            stale_liquifunding: None,
            stale_price: None,
            congested: false,
            fees: Fees {
                wallets: Default::default(),
                protocol: Default::default(),
                crank: Default::default(),
            },
        }
    }

    fn new_position_action(
        id: u64,
        kind: PositionActionKind,
        timestamp: u64,
        transfer_collateral: &str,
        owners: Option<(&Addr, &Addr)>,
    ) -> PositionAction {
        let old_owner = owners.map(|owners| owners.0.clone());
        let new_owner = owners.map(|owners| owners.1.clone());

        PositionAction {
            id: Some(PositionId::new(id)),
            kind,
            timestamp: Timestamp::from_seconds(timestamp),
            collateral: Default::default(),
            transfer_collateral: Signed::<Collateral>::from_str(transfer_collateral).unwrap(),
            leverage: None,
            max_gains: None,
            trade_fee: None,
            delta_neutrality_fee: None,
            old_owner,
            new_owner,
            take_profit_override: None,
            stop_loss_override: None,
        }
    }

    fn new_lp_action(kind: LpActionKind, timestamp: u64, collateral: &str) -> LpAction {
        LpAction {
            kind,
            timestamp: Timestamp::from_seconds(timestamp),
            tokens: None,
            collateral: collateral.parse().unwrap(),
            collateral_usd: Default::default(),
        }
    }

    #[test]
    fn test_export_history() {
        let start = 1687651200;
        let old_owner = Addr::unchecked("old-owner");
        let new_owner = Addr::unchecked("new-owner");
        let status = new_status_resp();
        let position_actions = vec![
            new_position_action(1u64, PositionActionKind::Open, start + 60, "10", None),
            new_position_action(2u64, PositionActionKind::Update, start + 120, "-5", None),
            new_position_action(2u64, PositionActionKind::Update, start + 150, "0", None), // this should not show up
            new_position_action(3u64, PositionActionKind::Close, start + 180, "15", None),
            new_position_action(
                4u64,
                PositionActionKind::Transfer,
                start + 240,
                "15",
                Some((&old_owner, &new_owner)),
            ),
            new_position_action(
                5u64,
                PositionActionKind::Transfer,
                start + 300,
                "15",
                Some((&new_owner, &old_owner)),
            ),
        ];
        let directions: HashMap<PositionId, DirectionToBase> = [
            (PositionId::new(1), DirectionToBase::Long),
            (PositionId::new(2), DirectionToBase::Short),
            (PositionId::new(3), DirectionToBase::Long),
            (PositionId::new(4), DirectionToBase::Short),
            (PositionId::new(5), DirectionToBase::Long),
        ]
        .into();
        let lp_actions = vec![
            new_lp_action(LpActionKind::DepositLp, start + 90, "1000"),
            new_lp_action(LpActionKind::DepositXlp, start + 150, "2000"),
            new_lp_action(LpActionKind::ReinvestYieldLp, start + 210, "3000"),
            new_lp_action(LpActionKind::ReinvestYieldXlp, start + 270, "4000"),
            new_lp_action(LpActionKind::UnstakeXlp, start + 330, "5000"), // this should not show up
            new_lp_action(LpActionKind::CollectLp, start + 360, "6000"),
            new_lp_action(LpActionKind::Withdraw, start + 390, "7000"),
            new_lp_action(LpActionKind::ClaimYield, start + 420, "8000"),
        ];

        let records = get_action_records(
            status,
            &position_actions,
            directions,
            &lp_actions,
            &old_owner.into(),
        )
        .unwrap();
        let csv = generate_csv(&records).unwrap();
        let expected = "Transaction Time (UTC),Position ID,Market,Direction,Action,Asset,Amount\n\
            2023-06-25 00:01:00,1,BASE+-QUOTE,Long,Open,BASE,10\n\
            2023-06-25 00:01:30,-,BASE+-QUOTE,-,Deposit LP,BASE,1000\n\
            2023-06-25 00:02:00,2,BASE+-QUOTE,Short,Update,BASE,-5\n\
            2023-06-25 00:02:30,-,BASE+-QUOTE,-,Deposit xLP,BASE,2000\n\
            2023-06-25 00:03:00,3,BASE+-QUOTE,Long,Close,BASE,15\n\
            2023-06-25 00:03:30,-,BASE+-QUOTE,-,Reinvest Yield LP,BASE,3000\n\
            2023-06-25 00:04:00,4,BASE+-QUOTE,Short,Sent Position,BASE,15\n\
            2023-06-25 00:04:30,-,BASE+-QUOTE,-,Reinvest Yield xLP,BASE,4000\n\
            2023-06-25 00:05:00,5,BASE+-QUOTE,Long,Received Position,BASE,15\n\
            2023-06-25 00:06:00,-,BASE+-QUOTE,-,Convert xLP-LP,BASE,6000\n\
            2023-06-25 00:06:30,-,BASE+-QUOTE,-,Withdraw LP,BASE,7000\n\
            2023-06-25 00:07:00,-,BASE+-QUOTE,-,Claim Yield,BASE,8000\n";

        assert_eq!(csv, expected);
    }
}
