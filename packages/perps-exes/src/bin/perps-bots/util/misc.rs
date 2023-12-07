use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::Utc;
use cosmos::{Address, CosmosTxResponse};

use crate::app::{App, FundUsed, FundsCoin};

pub(crate) async fn track_tx_fees(app: &App, addr: Address, response: &CosmosTxResponse) {
    let auth_info = response.tx.auth_info.clone();
    if let Some(auth_info) = auth_info {
        if let Some(fee) = auth_info.fee {
            let funds: Result<Vec<FundsCoin>> =
                fee.amount.into_iter().map(FundsCoin::try_from).collect();
            match funds {
                Ok(funds) => {
		    let funds = funds.iter().map(|item| item.amount.clone()).sum::<BigDecimal>();
                    let mut funds_used = app.funds_used.write().await;
                    funds_used
                        .entry(addr)
                        .or_insert_with(|| FundUsed {
                            total: Default::default(),
                            entries: Default::default(),
                            usage_per_hour: Default::default(),
                        })
                        .add_entry(Utc::now(), funds);
                }
                Err(e) => tracing::error!("Error converting coins to fundscoin: {e}"),
            }
        }
    }
}
