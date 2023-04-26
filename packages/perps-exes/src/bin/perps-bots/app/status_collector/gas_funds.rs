use std::sync::Arc;

use anyhow::{Context, Result};
use cosmos::{Address, Coin, Cosmos, Wallet};
use tokio::sync::Mutex;

use crate::app::status_collector::{StatusCategory, StatusCollector};

use super::Status;

/// Starts a new background task that regularly checks the amount of gas funds.
impl StatusCollector {
    pub(crate) fn track_gas_funds(
        &self,
        address: Address,
        wallet_name: impl Into<String>,
        min_amount: u128,
        gas_wallet: Arc<Mutex<Wallet>>,
    ) {
        let cosmos = self.cosmos.clone();
        self.add_status_check(
            StatusCategory::GasCheck,
            wallet_name.into(),
            UPDATE_LOOP_SECONDS,
            move || check_once(cosmos.clone(), address, min_amount, gas_wallet.clone()),
        )
    }

    /// One-off ensure that there is the given amount of gas
    pub(crate) async fn ensure_gas(
        &self,
        cosmos: Cosmos,
        address: Address,
        min_amount: u128,
        gas_wallet: Arc<Mutex<Wallet>>,
    ) -> Result<()> {
        match check_once_inner(&cosmos, address).await {
            Ok(amount) => {
                if amount < min_amount {
                    match tap_faucet(address, &cosmos, gas_wallet, min_amount).await {
                    Err(e) => Err(anyhow::anyhow!("Too little gas funds in wallet {address}. Wanted at least {min_amount}, but have {amount}. Tried tapping facuet, but got {e:?}")),
                    Ok(false) => Err(anyhow::anyhow!("Too little gas funds in wallet {address}. Wanted at least {min_amount}, but have {amount}")),
                    Ok(true) => match check_once_inner(&cosmos, address).await {
                        Ok(amount) => if amount < min_amount {
                            Err(anyhow::anyhow!("Too little gas funds in wallet {address} after tapping. Wanted at least {min_amount}, but have {amount}"))
                        } else {
                            Ok(())
                        },
                        Err(e) => Err(anyhow::anyhow!("Failure loading gas amount for {address} after tapping faucet: {e:?}")),
                    }
                }
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(anyhow::anyhow!(
                "Failure loading gas amount for {address}: {e:?}"
            )),
        }
    }
}

async fn check_once(
    cosmos: Cosmos,
    address: Address,
    min_amount: u128,
    gas_wallet: Arc<Mutex<Wallet>>,
) -> Status {
    match check_once_inner(&cosmos, address).await {
        Ok(amount) => {
            if amount < min_amount {
                match tap_faucet(address, &cosmos, gas_wallet, min_amount).await {
                    Err(e) => Status::error(format!("Too little gas funds in wallet {address}. Wanted at least {min_amount}, but have {amount}. Tried tapping facuet, but got {e:?}")),
                    Ok(false) => Status::error(format!("Too little gas funds in wallet {address}. Wanted at least {min_amount}, but have {amount}")),
                    Ok(true) => match check_once_inner(&cosmos, address).await {
                        Ok(amount) => if amount < min_amount {
                            Status::error(format!("Too little gas funds in wallet {address} after tapping. Wanted at least {min_amount}, but have {amount}"))
                        } else {
                            Status::success(format!("Tapped the faucet, have enough funds in wallet {address}, currently have {amount}"), Some(TOO_OLD_SECONDS))
                        },
                        Err(e) => Status::error(format!("Failure loading gas amount for {address} after tapping faucet: {e:?}")),
                    }
                }
            } else {
                Status::success(
                    format!("Enough funds in wallet {address}, currently have {amount}"),
                    Some(TOO_OLD_SECONDS),
                )
            }
        }
        Err(e) => Status::error(format!("Failure loading gas amount for {address}: {e:?}")),
    }
}

async fn tap_faucet(
    address: Address,
    cosmos: &Cosmos,
    gas_wallet: Arc<Mutex<Wallet>>,
    min_amount: u128,
) -> Result<bool> {
    let gas_wallet = gas_wallet.lock().await;
    log::info!("Attempting to refill gas from {gas_wallet} to {address}, sending {min_amount}");
    let res = gas_wallet
        .send_gas_coin(cosmos, &address, min_amount)
        .await?;
    log::info!("Refilled in {}", res.txhash);
    Ok(true)
}

async fn check_once_inner(cosmos: &Cosmos, address: Address) -> Result<u128> {
    let coins = cosmos.all_balances(address).await?;
    for Coin { denom, amount } in coins {
        if &denom == cosmos.get_gas_coin() {
            return amount
                .parse()
                .with_context(|| format!("Invalid gas coin amount {amount:?}"));
        }
    }
    Ok(0)
}

const UPDATE_LOOP_SECONDS: u64 = 60;
const TOO_OLD_SECONDS: i64 = 180;
