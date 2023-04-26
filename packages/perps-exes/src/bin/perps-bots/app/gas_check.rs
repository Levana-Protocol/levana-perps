use std::{collections::HashSet, sync::Arc};

use crate::{
    app::App,
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};
use anyhow::{Context, Result};
use axum::async_trait;
use cosmos::{
    proto::cosmos::bank::v1beta1::MsgSend, Address, Coin, Cosmos, HasAddress, TxBuilder, Wallet,
};

use super::AppBuilder;

pub(crate) struct GasCheckBuilder {
    tracked_wallets: HashSet<Address>,
    tracked_names: HashSet<String>,
    to_track: Vec<Tracked>,
    gas_wallet: Arc<Wallet>,
}

impl GasCheckBuilder {
    pub(crate) fn new(gas_wallet: Arc<Wallet>) -> GasCheckBuilder {
        GasCheckBuilder {
            tracked_wallets: Default::default(),
            tracked_names: Default::default(),
            to_track: Default::default(),
            gas_wallet,
        }
    }

    pub(crate) fn add(
        &mut self,
        address: Address,
        name: impl Into<String>,
        min_gas: u128,
        should_refill: bool,
    ) -> Result<()> {
        let name = name.into();
        anyhow::ensure!(
            self.tracked_names.insert(name.clone()),
            "Wallet name already in use: {name}"
        );
        anyhow::ensure!(
            self.tracked_wallets.insert(address),
            "Address already being tracked: {address}"
        );
        self.to_track.push(Tracked {
            name,
            address,
            min_gas,
            should_refill,
        });
        Ok(())
    }

    pub(crate) fn get_wallet_address(&self) -> Address {
        *self.gas_wallet.address()
    }

    pub(crate) fn build(&mut self, app: Arc<App>) -> GasCheck {
        GasCheck {
            to_track: std::mem::take(&mut self.to_track),
            gas_wallet: self.gas_wallet.clone(),
            app,
        }
    }
}

pub(crate) struct GasCheck {
    to_track: Vec<Tracked>,
    gas_wallet: Arc<Wallet>,
    app: Arc<App>,
}

impl AppBuilder {
    pub(crate) async fn launch_gas_task(&mut self, gas_check: GasCheck) -> Result<()> {
        // Do an initial gas check to make sure everything is OK
        gas_check.single_gas_check().await?;

        self.watch_periodic(crate::watcher::TaskLabel::GasCheck, gas_check)
    }
}

#[async_trait]
impl WatchedTask for GasCheck {
    async fn run_single(&self, _app: &App, _heartbeat: Heartbeat) -> Result<WatchedTaskOutput> {
        self.single_gas_check().await
    }
}

impl GasCheck {
    async fn single_gas_check(&self) -> Result<WatchedTaskOutput> {
        let mut errors = vec![];
        let mut to_refill = vec![];
        for Tracked {
            name,
            address,
            min_gas,
            should_refill,
        } in &self.to_track
        {
            let gas = get_gas_balance(&self.app.cosmos, *address).await?;
            if gas >= *min_gas {
                continue;
            }

            if *should_refill {
                to_refill.push((*address, *min_gas));
            } else {
                errors.push(format!(
                    "Insufficient gas in {name} ({address}). Found: {gas}. Wanted: {min_gas}."
                ));
            }
        }

        if !to_refill.is_empty() {
            let mut builder = TxBuilder::default();
            let denom = self.app.cosmos.get_gas_coin();
            for (address, amount) in to_refill {
                builder.add_message_mut(MsgSend {
                    from_address: self.gas_wallet.get_address_string(),
                    to_address: address.get_address_string(),
                    amount: vec![Coin {
                        denom: denom.clone(),
                        amount: amount.to_string(),
                    }],
                })
            }

            match builder
                .sign_and_broadcast(&self.app.cosmos, &self.gas_wallet)
                .await
            {
                Err(e) => {
                    log::error!("Error filling up gas: {e:?}");
                    errors.push(format!("{e:?}"))
                }
                Ok(tx) => {
                    log::info!("Filled up gas in {}", tx.txhash);
                }
            }
        }

        if errors.is_empty() {
            Ok(WatchedTaskOutput {
                message: format!("Enough gas in all wallets, checked {}", self.to_track.len()),
                skip_delay: false,
            })
        } else {
            let errors = errors.join("\n");
            Err(anyhow::anyhow!("{errors}"))
        }
    }
}

struct Tracked {
    name: String,
    address: Address,
    min_gas: u128,
    should_refill: bool,
}

async fn get_gas_balance(cosmos: &Cosmos, address: Address) -> Result<u128> {
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
