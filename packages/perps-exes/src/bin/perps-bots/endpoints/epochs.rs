use std::{fmt::Display, iter::Sum, ops::AddAssign, sync::Arc};

use axum::Extension;
use chrono::{DateTime, Utc};
use cosmos::proto::{
    cosmos::base::abci::v1beta1::TxResponse,
    tendermint::abci::{Event, EventAttribute},
};
use parking_lot::RwLock;

use crate::app::App;

/// Tracks the timestamps for beginning and ending of epoch processing.
#[derive(Clone, Default)]
pub(crate) struct Epochs {
    /// Assumes just one market is running, if this horrible code survives long
    /// enough turn into a `HashMap` from market or something like that.
    inner: Arc<RwLock<EpochsInner>>,
}

impl Epochs {
    /// Log that the epoch is currently running
    pub(crate) fn log_active(&self) {
        let mut inner = self.inner.write();
        if inner.current.is_none() {
            inner.current = Some(Current::new());
        }
    }

    /// Log that the epoch is currently not running
    pub(crate) fn log_inactive(&self) {
        let mut inner = self.inner.write();
        if let Some(Current { trans, start }) = inner.current.take() {
            inner.prior.push(Prior {
                trans,
                start,
                end: Utc::now(),
            });
        }
    }

    /// Log messages received during a transaction to count stats
    pub(crate) fn log_stats(&self, txres: &TxResponse) {
        let trans = Transaction::from(txres);
        let mut inner = self.inner.write();
        if inner.current.is_none() {
            inner.current = Some(Current::new());
        }
        inner.current.as_mut().unwrap().trans.push(trans);
    }
}

#[derive(Default)]
struct EpochsInner {
    prior: Vec<Prior>,
    current: Option<Current>,
}

struct Prior {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    trans: Vec<Transaction>,
}

struct Current {
    start: DateTime<Utc>,
    trans: Vec<Transaction>,
}

impl Current {
    fn new() -> Self {
        Current {
            start: Utc::now(),
            trans: vec![],
        }
    }
}

#[derive(Default)]
struct Stats {
    liquifunding: u32,
    liquidated: u32,
    take_profit: u32,
    gas_used: i64,
}

struct Transaction {
    txhash: String,
    stats: Stats,
}

pub(crate) async fn show_epochs(app: Extension<App>) -> String {
    app.get_epochs().inner.read().to_string()
}

impl Display for EpochsInner {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.current {
            None => writeln!(f, "No current epoch running")?,
            Some(Current { start, trans }) => {
                writeln!(
                    f,
                    "Current epoch started at {start}, running for {seconds} seconds.",
                    seconds = seconds(*start, Utc::now()),
                )?;
                write_transactions(trans, f)?;
            }
        }
        writeln!(f)?;

        for prior in self.prior.iter().rev() {
            writeln!(f, "{prior}")?;
            writeln!(f)?;
        }

        Ok(())
    }
}

fn seconds(start: DateTime<Utc>, end: DateTime<Utc>) -> i64 {
    end.timestamp() - start.timestamp()
}

impl Display for Prior {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(
            f,
            "Started: {start}. Ended: {end}. Ran for: {seconds} seconds.",
            start = self.start,
            end = self.end,
            seconds = seconds(self.start, self.end),
        )?;
        write_transactions(&self.trans, f)
    }
}

fn write_transactions(trans: &[Transaction], f: &mut std::fmt::Formatter) -> std::fmt::Result {
    let total: Stats = trans.iter().map(|t| &t.stats).sum();
    writeln!(f, "Total: {total}")?;
    for Transaction { txhash, stats } in trans {
        writeln!(f, "{txhash}: {stats}")?;
    }
    Ok(())
}

impl Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Stats {
            liquifunding,
            liquidated,
            take_profit,
            gas_used,
        } = self;
        write!(f, "Gas: {gas_used}. Liquifundings: {liquifunding}. Liquidated: {liquidated}. Take profit: {take_profit}")
    }
}

impl<'a> Sum<&'a Stats> for Stats {
    fn sum<I: Iterator<Item = &'a Stats>>(iter: I) -> Self {
        let mut res = Stats::default();
        for x in iter {
            res += x;
        }
        res
    }
}

impl From<&TxResponse> for Transaction {
    fn from(tx: &TxResponse) -> Self {
        let mut stats = Stats {
            gas_used: tx.gas_used,
            ..Stats::default()
        };
        for Event { r#type, attributes } in &tx.events {
            for EventAttribute {
                key,
                value,
                index: _,
            } in attributes
            {
                #[allow(clippy::single_match)]
                match (std::str::from_utf8(key), std::str::from_utf8(value)) {
                    (Ok(key), Ok(value)) => stats.add_event(r#type, key, value),
                    _ => (),
                }
            }
        }
        Transaction {
            txhash: tx.txhash.clone(),
            stats,
        }
    }
}

impl Stats {
    fn add_event(&mut self, typ: &str, key: &str, value: &str) {
        match (typ, key) {
            ("wasm-crank-exec", "liquifunding-len") => {
                if let Ok(len) = value.parse::<u32>() {
                    self.liquifunding += len;
                }
            }
            ("wasm-position-close", "close-reason") => {
                if value == "liquidated" {
                    self.liquidated += 1;
                } else if value == "take-profit" {
                    self.take_profit += 1;
                }
            }
            _ => (),
        }
    }
}

impl AddAssign<&Stats> for Stats {
    fn add_assign(
        &mut self,
        Stats {
            liquifunding,
            liquidated,
            take_profit,
            gas_used,
        }: &Stats,
    ) {
        self.liquifunding += liquifunding;
        self.liquidated += liquidated;
        self.take_profit += take_profit;
        self.gas_used += gas_used;
    }
}
