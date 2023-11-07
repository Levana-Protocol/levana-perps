use anyhow::{Context, Result};
use shared::storage::MarketId;
use sqlx::postgres::PgPoolOptions;
use sqlx::{query_as, query_scalar, PgPool};

use crate::types::*;

use super::super::endpoints::pnl::PositionInfo;
use super::models::{PositionInfoFromDb, PositionInfoToDb};

#[derive(Clone)]
pub(crate) struct Db {
    pub(crate) pool: PgPool,
}

impl Db {
    pub async fn new(uri: &str) -> Result<Db> {
        let pool = PgPoolOptions::new().max_connections(5).connect(uri).await?;
        Ok(Db { pool })
    }

    pub(crate) async fn get_or_insert_market(
        &self,
        address: &str,
        chain: ChainId,
        market_id: MarketId,
        environment: ContractEnvironment,
    ) -> Result<i64> {
        if let Some(market_id) =
            sqlx::query_scalar!(r#"SELECT id FROM market WHERE address=$1"#, address)
                .fetch_optional(&self.pool)
                .await?
        {
            Ok(market_id)
        } else {
            sqlx::query_scalar!(
                r#"
                    INSERT INTO market(address, chain, market_id, environment)
                    VALUES($1, $2, $3, $4)
                    RETURNING id"#,
                address,
                chain as i32,
                market_id.to_string().replace('_', "/"),
                environment as i32,
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.into())
        }
    }

    pub(crate) async fn insert_position_detail(
        &self,
        PositionInfoToDb {
            info:
                PositionInfo {
                    address,
                    chain,
                    position_id,
                    pnl_type,
                    display_wallet,
                },
            market_id,
            pnl_usd,
            pnl_percentage,
            direction,
            entry_price,
            exit_price,
            leverage,
            environment,
            wallet,
        }: PositionInfoToDb,
    ) -> Result<i64> {
        let market = self
            .get_or_insert_market(&address.to_string(), chain, market_id, environment)
            .await?;
        let position_u64 = position_id.u64();
        let position_id =
            i64::try_from(position_u64).context("Error converting {position_u64} to i64 type")?;
        let url_id = query_scalar!(
            r#"
                INSERT INTO position_detail
                (market, position_id, pnl_usd, pnl_percentage, direction, entry_price, exit_price, leverage, pnl_type, wallet)
                VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                RETURNING url_id
            "#,
            market,
            position_id,
            pnl_usd,
            pnl_percentage,
            direction as i32,
            TwoDecimalPoints(entry_price.into_number()).to_string(),
            TwoDecimalPoints(exit_price.into_number()).to_string(),
            leverage,
            pnl_type as i32,
            wallet,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(url_id)
    }

    pub(crate) async fn get_url_detail(&self, url_id: i64) -> Result<Option<PositionInfoFromDb>> {
        query_as!(
            PositionInfoFromDb,
            r#"
                SELECT
                    market_id,
                    pnl_usd,
                    pnl_percentage,
                    entry_price,
                    exit_price,
                    leverage,
                    direction as "direction: DirectionForDb",
                    environment as "environment: ContractEnvironment",
                    chain as "chain: ChainId",
                    wallet
                FROM position_detail INNER JOIN market
                ON position_detail.market = market.id
                WHERE url_id=$1
            "#,
            url_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.into())
    }
}
