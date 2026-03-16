use crate::types::prelude::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PendingRawTransactionModel;

impl PendingRawTransactionModel {
    pub const ID: &'static str = stringify!(PendingRawTransactionModel);
    const COUNTER_SUFFIX: &'static str = "pending_counter";

    pub fn next_index(rollup_id: &RollupId) -> Result<u64, KvStoreError> {
        let counter_key = &(Self::ID, rollup_id, Self::COUNTER_SUFFIX);

        // Try to get a mutable lock on the existing counter.
        // This ensures that read-modify-write of the counter is atomic.
        if let Ok(mut counter_lock) = kvstore()?.get_mut(counter_key) {
            let current: u64 = *counter_lock;
            *counter_lock = current.saturating_add(1);
            Ok(current)
        } else {
            // If the counter does not exist yet, initialize it to 1 and return 0
            // as the first index. There is a tiny race window only on first use,
            // but after initialization the get_mut path is used and is atomic.
            kvstore()?.put(counter_key, &1u64)?;
            Ok(0)
        }
    }

    pub fn enqueue(
        rollup_id: &RollupId,
        raw_transaction: RawTransaction,
    ) -> Result<u64, KvStoreError> {
        let index = Self::next_index(rollup_id)?;
        let key = &(Self::ID, rollup_id, index);
        kvstore()?.put(key, &raw_transaction)?;
        Ok(index)
    }

    pub fn get(
        rollup_id: &RollupId,
        index: u64,
    ) -> Result<RawTransaction, KvStoreError> {
        let key = &(Self::ID, rollup_id, index);
        kvstore()?.get(key)
    }

    pub fn delete(
        rollup_id: &RollupId,
        index: u64,
    ) -> Result<(), KvStoreError> {
        let key = &(Self::ID, rollup_id, index);
        kvstore()?.delete(key)
    }

    pub fn drain_start(rollup_id: &RollupId) -> Result<u64, KvStoreError> {
        let counter_key = &(Self::ID, rollup_id, "drain_start");
        Ok(kvstore()?.get(counter_key).unwrap_or(0))
    }

    pub fn set_drain_start(rollup_id: &RollupId, value: u64) -> Result<(), KvStoreError> {
        let counter_key = &(Self::ID, rollup_id, "drain_start");
        kvstore()?.put(counter_key, &value)
    }

    pub fn pending_count(rollup_id: &RollupId) -> Result<u64, KvStoreError> {
        let start = Self::drain_start(rollup_id)?;
        let counter_key = &(Self::ID, rollup_id, Self::COUNTER_SUFFIX);
        let end: u64 = kvstore()?.get(counter_key).unwrap_or(0);
        Ok(end.saturating_sub(start))
    }

    pub fn drain_all(rollup_id: &RollupId) -> Result<Vec<RawTransaction>, KvStoreError> {
        let start = Self::drain_start(rollup_id)?;
        let counter_key = &(Self::ID, rollup_id, Self::COUNTER_SUFFIX);
        let end: u64 = kvstore()?.get(counter_key).unwrap_or(0);

        let mut results = Vec::new();
        for i in start..end {
            match Self::get(rollup_id, i) {
                Ok(tx) => {
                    results.push(tx);
                    let _ = Self::delete(rollup_id, i);
                }
                Err(_) => continue,
            }
        }
        Self::set_drain_start(rollup_id, end)?;
        Ok(results)
    }
}
