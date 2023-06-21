use std::str::FromStr;

use anyhow::{bail, Context, Result};
use cosmos::Address;
use msg::contracts::market::position::PositionId;
use sqlx::postgres::PgPoolOptions;
use sqlx::{query_as, PgPool};

use crate::db::models::AddressModel;
use crate::endpoints::pnl::PnlType;
use crate::types::ChainId;

use super::super::endpoints::pnl::PositionInfo;
use super::models::{PositionDetail, UrlDetail};

#[derive(Clone)]
pub(crate) struct Db {
    pub(crate) pool: PgPool,
}

impl Db {
    pub async fn new(uri: &str) -> Result<Db> {
        let pool = PgPoolOptions::new().max_connections(5).connect(uri).await?;
        Ok(Db { pool })
    }

    pub(crate) async fn get_or_insert_address(&self, address: &str) -> Result<AddressModel> {
        if let Some(address) = sqlx::query_as!(
            AddressModel,
            r#"SELECT id, address FROM address WHERE address=$1"#,
            address
        )
        .fetch_optional(&self.pool)
        .await?
        {
            Ok(address)
        } else {
            sqlx::query_as!(
                AddressModel,
                r#"INSERT INTO address(address) VALUES($1) RETURNING id, address"#,
                address
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| e.into())
        }
    }

    pub(crate) async fn get_address_by_id(&self, id: i64) -> Result<AddressModel> {
        query_as!(
            AddressModel,
            "SELECT id, address FROM address WHERE id=$1",
            id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| e.into())
    }

    async fn check_existing_position_detail(
        &self,
        position_info: PositionInfo,
    ) -> Result<Option<PositionDetail>> {
        let address = query_as!(
            AddressModel,
            "SELECT id, address FROM address WHERE address=$1",
            &position_info.address.to_string()
        )
        .fetch_optional(&self.pool)
        .await?;
        let pnl: String = position_info.pnl_type.into();
        let position_u64 = position_info.position_id.u64();
        let position_id =
            i64::try_from(position_u64).context("Error converting {position_u64} to i64 type")?;
        match address {
            Some(address) => {
                let position_detail = query_as!(PositionDetail, r#"SELECT id, contract_address, chain as "chain: ChainId", position_id, pnl_type, url_id FROM position_detail WHERE contract_address=$1 AND chain=$2 AND position_id=$3 AND pnl_type=$4"#, address.id, position_info.chain as i32, position_id , pnl).fetch_optional(&self.pool).await?;
                Ok(position_detail)
            }
            None => Ok(None),
        }
    }

    pub(crate) async fn insert_position_detail(
        &self,
        position_info: PositionInfo,
    ) -> Result<PositionDetail> {
        let opt_postion_detail = self
            .check_existing_position_detail(position_info.clone())
            .await?;
        match opt_postion_detail {
            Some(position_detail) => Ok(position_detail),
            None => {
                let address = self
                    .get_or_insert_address(&position_info.address.to_string())
                    .await?;
                let chain = position_info.chain;
                let pnl: String = position_info.pnl_type.into();
                let position_u64 = position_info.position_id.u64();
                let position_id = i64::try_from(position_u64)
                    .context("Error converting {position_u64} to i64 type")?;
                let position_detail = query_as!(PositionDetail, r#"INSERT INTO position_detail(contract_address, chain, position_id, pnl_type) VALUES($1, $2, $3, $4) RETURNING id, contract_address, chain as "chain: ChainId", position_id, pnl_type, url_id"#, address.id, chain as i32, position_id, pnl).fetch_one(&self.pool).await?;
                Ok(position_detail)
            }
        }
    }

    pub(crate) async fn get_url_detail(&self, url_id: i32) -> Result<Option<UrlDetail>> {
        let result = query_as!(PositionDetail, r#"SELECT id, contract_address, chain as "chain: ChainId", position_id, pnl_type, url_id FROM position_detail WHERE url_id=$1"#, url_id).fetch_optional(&self.pool).await?;
        match result {
            Some(position_detail) => {
                let id = position_detail.id;
                let contract_address = self
                    .get_address_by_id(position_detail.contract_address)
                    .await?
                    .address;
                let contract_address = Address::from_str(&contract_address)
                    .context(format!("Invalid address: {contract_address}"))?;
                let chain = position_detail.chain;
                let position_id = position_detail.position_id;
                let position_id = u64::try_from(position_id)
                    .context(format!("Error conversion to u64: {position_id}"))?;
                let pnl_type = position_detail.pnl_type;
                let pnl_type = match pnl_type.as_str() {
                    "Usd" => PnlType::Usd,
                    "Percent" => PnlType::Percent,
                    _ => bail!("Invalid pnl_type. Got {pnl_type}"),
                };
                Ok(Some(UrlDetail {
                    id,
                    contract_address,
                    chain,
                    position_id: PositionId::new(position_id),
                    pnl_type,
                    url_id,
                }))
            }
            None => Ok(None),
        }
    }
}
