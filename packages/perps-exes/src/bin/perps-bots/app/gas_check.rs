use std::{
    collections::{HashSet, VecDeque},
    fmt::Display,
    sync::Arc,
};

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
use msg::prelude::{LpToken, UnsignedDecimal};

use super::AppBuilder;

pub(crate) struct GasCheckBuilder {
    tracked_wallets: HashSet<Address>,
    tracked_names: HashSet<GasCheckWallet>,
    to_track: Vec<Tracked>,
    gas_wallet: Option<Arc<Wallet>>,
}

/// Description of which wallet is being tracked
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GasCheckWallet {
    FaucetBot,
    FaucetContract,
    GasWallet,
    WalletManager,
    Crank,
    Price,
    Managed(ManagedWallet),
    UltraCrank(usize),
}

impl Display for GasCheckWallet {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            GasCheckWallet::FaucetBot => write!(f, "Faucet bot"),
            GasCheckWallet::FaucetContract => write!(f, "Faucet contract"),
            GasCheckWallet::GasWallet => write!(f, "Gas wallet"),
            GasCheckWallet::WalletManager => write!(f, "Wallet manager"),
            GasCheckWallet::Crank => write!(f, "Crank"),
            GasCheckWallet::Price => write!(f, "Price"),
            GasCheckWallet::Managed(x) => write!(f, "{x}"),
            GasCheckWallet::UltraCrank(x) => write!(f, "Ultra crank #{x}"),
        }
    }
}

impl GasCheckBuilder {
    pub(crate) fn new(gas_wallet: Option<Arc<Wallet>>) -> GasCheckBuilder {
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
        min_gas: u128,
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

    pub(crate) fn get_wallet_address(&self) -> Option<Address> {
        self.gas_wallet.as_ref().map(|x| *x.address())
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
    gas_wallet: Option<Arc<Wallet>>,
    app: Arc<App>,
}

impl AppBuilder {
    pub(crate) fn launch_gas_task(&mut self, gas_check: GasCheck) -> Result<()> {
        self.watch_periodic(crate::watcher::TaskLabel::GasCheck, gas_check)
    }
}

#[async_trait]
impl WatchedTask for GasCheck {
    async fn run_single(&mut self, app: &App, _heartbeat: Heartbeat) -> Result<WatchedTaskOutput> {
        self.single_gas_check(app).await
    }
}

fn pretty_gas(x: u128) -> Decimal256 {
    LpToken::from_u128(x)
        .ok()
        .map_or_else(Decimal256::zero, |x| x.into_decimal256())
}

impl GasCheck {
    async fn single_gas_check(&self, app: &App) -> Result<WatchedTaskOutput> {
        let mut balances = vec![];
        let mut errors = vec![];
        let mut to_refill = vec![];
        let mut skip_delay = false;
        let now = Utc::now();
        for Tracked {
            name,
            address,
            min_gas,
            should_refill,
        } in &self.to_track
        {
            let gas = match get_gas_balance(&self.app.cosmos, *address).await {
                Ok(gas) => gas,
                Err(e) => {
                    errors.push(format!("Unable to query gas balance for {address}: {e:?}"));
                    continue;
                }
            };
            let mut gases = app.gases.write();
            gases
                .entry(*address)
                .and_modify(|v| {
                    v.push_back((now, gas));
                    if v.len() >= 1000 {
                        v.pop_front();
                    }
                })
                .or_insert_with(|| {
                    let mut def = VecDeque::new();
                    def.push_back((now, gas));
                    def
                });
            if gas >= *min_gas {
                balances.push(format!(
                    "Sufficient gas in {name} ({address}). Found: {}.",
                    pretty_gas(gas)
                ));
                continue;
            }

            if *should_refill {
                to_refill.push((*address, *min_gas));
                balances.push(format!(
                    "Topping off gas in {name} ({address}). Found: {}. Wanted: {}.",
                    pretty_gas(gas),
                    pretty_gas(*min_gas)
                ));
                if to_refill.len() >= 20 {
                    balances.push("Already have 20 wallets to fill up, stopping there".to_owned());
                    skip_delay = true;
                    break;
                }
            } else {
                errors.push(format!(
                    "Insufficient gas in {name} ({address}). Found: {}. Wanted: {}.",
                    pretty_gas(gas),
                    pretty_gas(*min_gas)
                ));
            }
        }
        if !to_refill.is_empty() {
            let mut builder = TxBuilder::default();
            let denom = self.app.cosmos.get_gas_coin();
            let gas_wallet = self
                .gas_wallet
                .clone()
                .context("Cannot refill gas automatically on mainnet")?;
            {
                let mut gases = app.gases.write();
                for (address, amount) in to_refill {
                    builder.add_message_mut(MsgSend {
                        from_address: gas_wallet.get_address_string(),
                        to_address: address.get_address_string(),
                        amount: vec![Coin {
                            denom: denom.clone(),
                            amount: amount.to_string(),
                        }],
                    });
                    gases.entry(address).and_modify(|v| {
                        for tg in v.iter_mut() {
                            let (t, g) = *tg;
                            *tg = (t, g + amount);
                        }
                    });
                }
            }

            match builder
                .sign_and_broadcast(&self.app.cosmos, &gas_wallet)
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
                message: balances.join("\n"),
                skip_delay,
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
