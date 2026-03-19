use crate::types::prelude::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PendingRawTransactionModel;

impl PendingRawTransactionModel {
    pub const ID: &'static str = stringify!(PendingRawTransactionModel);
    const COUNTER_SUFFIX: &'static str = "pending_counter";

    pub fn next_index(rollup_id: &RollupId) -> Result<u64, KvStoreError> {
        let counter_key = &(Self::ID, rollup_id, Self::COUNTER_SUFFIX);

        // Store "last issued index".
        //
        // - If the key does not exist, initialize it to 0 and return 0.
        // - Otherwise, increment it and return the new value.
        //
        // We use u64::MAX as a sentinel default so that the first increment wraps to 0.
        let mut counter_lock = kvstore()?.get_mut_or(counter_key, || u64::MAX)?;
        let next = (*counter_lock).wrapping_add(1);
        *counter_lock = next;
        counter_lock.update()?;
        Ok(next)
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
        Ok(*kvstore()?.get_mut_or(counter_key, || 0)?)
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
        /*
        let start = match Self::drain_start(rollup_id) {
            Ok(v) => v,
            Err(_) => return Ok(Vec::new()),
        };
        */
        let start_counter_key = &(Self::ID, rollup_id, "drain_start");
        let start_lock = safe_get_mut_or(start_counter_key, || 0)?;

        let end_counter_key = &(Self::ID, rollup_id, Self::COUNTER_SUFFIX);
        let end: u64 = kvstore()?.get(end_counter_key).unwrap_or(0);

        let start = *start_lock;

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

fn safe_get_mut_or<K, V, F>(key: &K, default_fn: F) -> Result<Lock<'static, V>, KvStoreError>
where
    K: std::fmt::Debug + Serialize,
    V: std::fmt::Debug + serde::de::DeserializeOwned + Serialize,
    F: FnOnce() -> V,
{
    match kvstore()?.get_mut(key) {
        Ok(lock) => Ok(lock),
        Err(KvStoreError::NoneType) => {
            kvstore()?.put(key, &default_fn())?;
            kvstore()?.get_mut(key)
        }
        Err(e) => Err(e),
    }
}