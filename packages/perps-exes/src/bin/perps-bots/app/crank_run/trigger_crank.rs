//! Utility struct for allowing deduped triggering of a crank for markets, with fairness guarantees.
//!
//! There's one particular complexity in this implementation: timeouts. We want
//! to be able to wait on channels for up to X number of seconds. Unfortunately,
//! if a new work item shows up at almost exactly the same time as the timeout
//! triggers, we could end up missing values on the channel. To account for
//! this, we treat the communication channel very conservatively, and instead
//! use a mutex of the actual data as the true source of what work items need to
//! exist. You'll see comments below about this conservative nature. If this
//! comment doesn't make sense on its own, don't worry, the comments below
//! should clarify it.
//!
//! The overall invariant we try to retain is that, at the end of each function
//! run, if there is work on the `Arc<Mutex<Queue>>`, there is guaranteed to be
//! at least one value in the channel.

use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::Instant,
};

use anyhow::{bail, Result};
use async_channel::TrySendError;
use cosmos::Address;
use parking_lot::Mutex;
use shared::storage::MarketId;

use crate::app::CrankTriggerReason;

/// The sending side only, for price and crank watch bots to trigger a run.
#[derive(Clone)]
pub(crate) struct TriggerCrank {
    /// The actual queue of work
    queue: Arc<Mutex<Queue>>,
    /// Channel for telling workers there's more work to be done.
    send: async_channel::Sender<()>,
}

#[derive(Default)]
struct Queue {
    /// FIFO queue of the markets to crank
    fifo: VecDeque<(Address, MarketId, CrankTriggerReason, Instant)>,
    /// HashSet matching everything currently waiting to be worked on or in flight.
    ///
    /// This HashSet should contain everything currently in fifo, plus works
    /// items that have been popped off of fifo but not yet executed.
    set: HashSet<Address>,
    /// The number of active crank guards, used for sanity checking only
    crank_guards: usize,
}

enum PopResult {
    QueueIsEmpty,
    ValueFound {
        address: Address,
        market_id: MarketId,
        more_work_exists: bool,
        reason: Box<CrankTriggerReason>,
        queued: Instant,
    },
}

/// Ensures that only one crank runner is working on a given market at a time.
pub(crate) struct CrankGuard {
    queue: Arc<Mutex<Queue>>,
    address: Address,
}

impl Drop for CrankGuard {
    fn drop(&mut self) {
        let mut queue = self.queue.lock();
        let was_present = queue.set.remove(&self.address);
        assert!(was_present);
        assert!(queue.crank_guards > 0);
        queue.crank_guards -= 1;
    }
}

impl Queue {
    fn pop(&mut self) -> PopResult {
        assert_eq!(self.fifo.len() + self.crank_guards, self.set.len());
        match self.fifo.pop_front() {
            None => PopResult::QueueIsEmpty,
            Some((address, market_id, reason, queued)) => {
                assert!(self.set.contains(&address));
                self.crank_guards += 1;
                PopResult::ValueFound {
                    address,
                    market_id,
                    more_work_exists: !self.fifo.is_empty(),
                    reason: Box::new(reason),
                    queued,
                }
            }
        }
    }

    /// Returns true if a new value was added to the queue
    fn push(&mut self, address: Address, market_id: MarketId, reason: CrankTriggerReason) -> bool {
        assert_eq!(self.fifo.len() + self.crank_guards, self.set.len());
        if self.set.contains(&address) {
            false
        } else {
            self.fifo
                .push_back((address, market_id, reason, Instant::now()));
            self.set.insert(address);
            true
        }
    }
}

/// Both the sending and receiving side, used by the crank runners.
#[derive(Clone)]
pub(crate) struct CrankReceiver {
    pub(super) trigger: TriggerCrank,
    recv: async_channel::Receiver<()>,
}

impl TriggerCrank {
    #[tracing::instrument(skip_all)]
    pub(crate) async fn trigger_crank(
        &self,
        contract: Address,
        market_id: MarketId,
        reason: CrankTriggerReason,
    ) {
        let added = self.queue.lock().push(contract, market_id, reason);
        if added {
            match self.send.try_send(()) {
                Ok(()) => (),
                Err(TrySendError::Closed(())) => unreachable!(
                    "trigger_crank: send failed because channel closed, which should be impossible"
                ),
                Err(TrySendError::Full(())) => {
                    log::warn!("Highly unlikely trigger_crank with full channel. It's not necessarily a bug, but almost certainly is.")
                }
            }
        }
    }
}

pub(crate) struct CrankWorkItem {
    pub(crate) address: Address,
    pub(crate) id: MarketId,
    pub(crate) guard: CrankGuard,
    pub(crate) reason: CrankTriggerReason,
    pub(crate) queued: Instant,
    pub(crate) received: Instant,
}

impl CrankReceiver {
    pub(super) fn new() -> Self {
        let (send, recv) = async_channel::bounded(100);
        CrankReceiver {
            trigger: TriggerCrank {
                queue: Arc::new(Mutex::new(Queue::default())),
                send,
            },
            recv,
        }
    }

    pub(super) async fn receive_work(&self) -> Result<CrankWorkItem> {
        let work = self.recv.recv().await;
        match work {
            Ok(()) => {
                let work = self.trigger.queue.lock().pop();
                match work {
                    PopResult::QueueIsEmpty => {
                        bail!("Possible bug: Signaled about work item, but didn't receive any.")
                    }
                    PopResult::ValueFound {
                        address,
                        market_id,
                        more_work_exists,
                        reason,
                        queued,
                    } => {
                        // We have some work. If there's even more work available,
                        // enforce our invariant that we always have a value on the
                        // channel in such a case.
                        if more_work_exists && self.recv.is_empty() {
                            match self.trigger.send.try_send(()) {
                                Ok(()) => (),
                                Err(TrySendError::Closed(())) => {
                                    unreachable!("Resending on empty channel encountered closed")
                                }
                                Err(TrySendError::Full(())) => {
                                    log::warn!("Highly suspect, resending on empty channel encountered full")
                                }
                            }
                        }
                        Ok(CrankWorkItem {
                            address,
                            id: market_id,
                            guard: CrankGuard {
                                queue: self.trigger.queue.clone(),
                                address,
                            },

                            reason: *reason,
                            queued,
                            received: Instant::now(),
                        })
                    }
                }
            }
            Err(err) => unreachable!(
                "receive: impossible RecvError, all sending sides have been closed {err:?}"
            ),
        }
    }
}
