use crate::prelude::*;

pub fn sanity(storage: &dyn Storage, _env: &Env) {
    let last_processed_inc_queue_id = crate::state::LAST_PROCESSED_INC_QUEUE_ID
        .may_load(storage)
        .unwrap();
    if let Some(last_processed_inc_queue_id) = last_processed_inc_queue_id {
        for index in 0u64..=last_processed_inc_queue_id.u64() {
            let id = IncQueuePositionId::new(index);
            let item = crate::state::COLLATERAL_INCREASE_QUEUE
                .may_load(storage, &id)
                .unwrap()
                .unwrap();
            assert!(!item.status.pending());
        }
        let last_inserted = crate::state::LAST_INSERTED_INC_QUEUE_ID
            .may_load(storage)
            .unwrap()
            .unwrap();
        assert!(last_inserted.u64() >= last_processed_inc_queue_id.u64());
    }
    let last_processed_dec_queue_id = crate::state::LAST_PROCESSED_DEC_QUEUE_ID
        .may_load(storage)
        .unwrap();
    if let Some(last_processed_dec_queue_id) = last_processed_dec_queue_id {
        for index in 0u64..=last_processed_dec_queue_id.u64() {
            let id = DecQueuePositionId::new(index);
            let item = crate::state::COLLATERAL_DECREASE_QUEUE
                .may_load(storage, &id)
                .unwrap()
                .unwrap();
            assert!(!item.status.pending());
        }
        let last_inserted = crate::state::LAST_INSERTED_DEC_QUEUE_ID
            .may_load(storage)
            .unwrap()
            .unwrap();
        assert!(last_inserted.u64() >= last_processed_dec_queue_id.u64());
    }
}
