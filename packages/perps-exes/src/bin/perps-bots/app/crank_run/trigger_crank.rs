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
};

use async_channel::{RecvError, TrySendError};
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
    fifo: VecDeque<(Address, MarketId, CrankTriggerReason)>,
    /// HashSet matching everything in fifo for efficient checking
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
        reason: CrankTriggerReason,
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
            Some((address, market_id, reason)) => {
                assert!(self.set.contains(&address));
                self.crank_guards += 1;
                PopResult::ValueFound {
                    address,
                    market_id,
                    more_work_exists: !self.set.is_empty(),
                    reason,
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
            self.fifo.push_back((address, market_id, reason));
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

impl From<CrankTriggerReason> for String {
    fn from(reason: CrankTriggerReason) -> Self {
        match reason {
            CrankTriggerReason::MoreWorkFound => "More work found".to_owned(),
            CrankTriggerReason::MessageWaiting => "Message waiting".to_owned(),
            CrankTriggerReason::PriceUpdateTooOld(duration) => {
                format!("Last price update was too old (age: {duration})")
            }
            CrankTriggerReason::PriceUpdateWillTrigger => {
                "Price update will hit price triggers".to_owned()
            }
            CrankTriggerReason::NoPriceFound => "No price found in contract".to_owned(),
        }
    }
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

    pub(super) async fn receive_with_timeout(
        &self,
    ) -> Option<(Address, MarketId, CrankGuard, CrankTriggerReason)> {
        // This unfortunately requires more care than it seems like it should.
        // It's possible that the timeout used on receive will end up missing an
        // update. Therefore, we always recheck the queue after a we finish,
        // even if the timeout triggered.
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(MAX_WAIT_SECONDS),
            self.recv.recv(),
        )
        .await
        {
            // Timeout occurred, not an error, just keep going with our logic
            Err(_) => (),
            // Popped a value from the queue, all good
            Ok(Ok(())) => (),
            Ok(Err(RecvError)) => unreachable!(
                "receive_with_timeout: impossible RecvError, all sending sides have been closed"
            ),
        }

        // OK, we're done waiting. Try to pop a value from the queue.
        match self.trigger.queue.lock().pop() {
            PopResult::QueueIsEmpty => {
                // No work item found, so return None and don't do anything to the channel.
                None
            }
            PopResult::ValueFound {
                address,
                more_work_exists,
                market_id,
                reason,
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
                        Err(TrySendError::Full(())) => log::warn!(
                            "Highly suspect, resending on empty channel encountered full"
                        ),
                    }
                }
                Some((
                    address,
                    market_id,
                    CrankGuard {
                        queue: self.trigger.queue.clone(),
                        address,
                    },
                    reason,
                ))
            }
        }
    }
}

const MAX_WAIT_SECONDS: u64 = 20;
