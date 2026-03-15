use crate::types::prelude::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PendingRawTransactionModel;

impl PendingRawTransactionModel {
    pub const ID: &'static str = stringify!(PendingRawTransactionModel);
    const COUNTER_SUFFIX: &'static str = "pending_counter";

    pub fn next_index(rollup_id: &RollupId) -> Result<u64, KvStoreError> {
        let counter_key = &(Self::ID, rollup_id, Self::COUNTER_SUFFIX);
        let current: u64 = kvstore()?.get(counter_key).unwrap_or(0);
        kvstore()?.put(counter_key, &(current + 1))?;
        Ok(current)
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
