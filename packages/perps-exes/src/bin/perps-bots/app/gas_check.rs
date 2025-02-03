use std::{collections::HashSet, fmt::Display, sync::Arc};

use crate::{
    app::App,
    wallet_manager::ManagedWallet,
    watcher::{Heartbeat, WatchedTask, WatchedTaskOutput},
};
use anyhow::{Context, Result};
use axum::async_trait;
use chrono::Utc;
use cosmos::{
    proto::cosmos::bank::v1beta1::MsgSend, Address, Coin, Cosmos, HasAddress, TxBuilder, Wallet,
};
use cosmwasm_std::Decimal256;
use perps_exes::config::{GasAmount, GasDecimals};
use serde::Serialize;

use super::{AppBuilder, GasRecords};

pub(crate) struct GasCheckBuilder {
    tracked_wallets: HashSet<Address>,
    tracked_names: HashSet<GasCheckWallet>,
    to_track: Vec<Tracked>,
    gas_wallet: Arc<Wallet>,
}

/// Description of which wallet is being tracked
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub(crate) enum GasCheckWallet {
    FaucetContract,
    GasWallet,
    WalletManager,
    Managed(ManagedWallet),
    HighGas,
    Pool(usize),
}

impl Display for GasCheckWallet {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            GasCheckWallet::FaucetContract => write!(f, "Faucet contract"),
            GasCheckWallet::GasWallet => write!(f, "Gas wallet"),
            GasCheckWallet::WalletManager => write!(f, "Wallet manager"),
            GasCheckWallet::Managed(x) => write!(f, "{x}"),
            GasCheckWallet::HighGas => write!(f, "High gas"),
            GasCheckWallet::Pool(x) => write!(f, "Pool {x}"),
        }
    }
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
        name: GasCheckWallet,
        min_gas: GasAmount,
        should_refill: bool,
    ) -> Result<()> {
        anyhow::ensure!(
            self.tracked_names.insert(name),
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
    pub(crate) fn start_gas_task(&mut self, gas_check: GasCheck) -> Result<()> {
        self.watch_periodic(crate::watcher::TaskLabel::GasCheck, gas_check)
    }
}

#[async_trait]
impl WatchedTask for GasCheck {
    async fn run_single(
        &mut self,
        app: Arc<App>,
        _heartbeat: Heartbeat,
    ) -> Result<WatchedTaskOutput> {
        self.single_gas_check(&app).await
    }
}

impl GasCheck {
    async fn single_gas_check(&self, app: &App) -> Result<WatchedTaskOutput> {
        let mut balances = vec![];
        let mut errors = vec![];
        let mut to_refill = vec![];
        let mut skip_delay = false;
        let now = Utc::now();
        let cosmos = &app.cosmos_gas_check;
        let mut total = GasAmount(Decimal256::zero());
        for Tracked {
            name,
            address,
            min_gas,
            should_refill,
        } in &self.to_track
        {
            let gas = match get_gas_balance(cosmos, *address, self.app.config.gas_decimals).await {
                Ok(gas) => gas,
                Err(e) => {
                    errors.push(format!("Unable to query gas balance for {address}: {e}"));
                    continue;
                }
            };
            total += gas;
            if gas >= *min_gas {
                balances.push(format!(
                    "Sufficient gas in {name} ({address}). Found: {gas}. Minimum: {min_gas}."
                ));
                continue;
            }

            if *should_refill {
                to_refill.push((*address, *min_gas, *name));
                balances.push(format!(
                    "Topping off gas in {name} ({address}). Found: {gas}. Wanted: {min_gas}."
                ));
                if to_refill.len() >= 20 {
                    balances.push("Already have 20 wallets to fill up, stopping there".to_owned());
                    skip_delay = true;
                    break;
                }
            } else {
                errors.push(format!(
                    "Insufficient gas in {name} ({address}). Found: {gas}. Wanted: {min_gas}."
                ));
            }
        }
        balances.push(format!("Total gas in all wallets: {total}"));
        if !to_refill.is_empty() {
            let mut builder = TxBuilder::default();
            let denom = cosmos.get_cosmos_builder().gas_coin();
            let gas_wallet = self.gas_wallet.clone();
            {
                for (address, amount, _) in &to_refill {
                    builder.add_message(MsgSend {
                        from_address: gas_wallet.get_address_string(),
                        to_address: address.get_address_string(),
                        amount: vec![Coin {
                            denom: denom.to_owned(),
                            amount: app.config.gas_decimals.to_u128(*amount)?.to_string(),
                        }],
                    });

                    // Temporary limitation: only a single Send can be sent in a tx.
                    if address.hrp().as_str() == "sthor" {
                        break;
                    }
                }
            }

            match builder.sign_and_broadcast(cosmos, &gas_wallet).await {
                Err(e) => {
                    tracing::error!("Error filling up gas: {e}");
                    errors.push(format!("{e}"))
                }
                Ok(tx) => {
                    tracing::info!("Filled up gas in {}", tx.txhash);
                    let mut gases = app.gas_refill.write().await;
                    for (address, amount, name) in to_refill {
                        gases
                            .entry(address)
                            .or_insert_with(|| GasRecords {
                                total: Default::default(),
                                entries: Default::default(),
                                wallet_type: name,
                                usage_per_hour: Default::default(),
                            })
                            .add_entry(now, amount);
                    }
                }
            }
        }

        if errors.is_empty() {
            let output = WatchedTaskOutput::new(balances.join("\n"));
            Ok(if skip_delay {
                output.skip_delay()
            } else {
                output
            })
        } else {
            errors.append(&mut balances);
            let errors = errors.join("\n");
            Err(anyhow::anyhow!("{errors}"))
        }
    }
}

struct Tracked {
    name: GasCheckWallet,
    address: Address,
    min_gas: GasAmount,
    should_refill: bool,
}

async fn get_gas_balance(
    cosmos: &Cosmos,
    address: Address,
    decimals: GasDecimals,
) -> Result<GasAmount> {
    let coins = cosmos.all_balances(address).await?;
    for Coin { denom, amount } in coins {
        if denom == cosmos.get_cosmos_builder().gas_coin() {
            let raw = amount
                .parse()
                .with_context(|| format!("Invalid gas coin amount {amount:?}"))?;
            return decimals.from_u128(raw);
        }
    }
    Ok(GasAmount(Decimal256::zero()))
}
