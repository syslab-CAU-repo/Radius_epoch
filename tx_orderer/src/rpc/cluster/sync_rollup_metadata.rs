/*
03.05 수정사항: completed_batch_number 대신 max_contiguous 사용함
sync_rollup_metadata 요청은 더이상 쓰이지 않으므로 고치지 않고 전체 주석 처리

use std::time::{SystemTime, UNIX_EPOCH};

use radius_sdk::json_rpc::server::ProcessPriority;

use super::LeaderChangeMessage;
use crate::rpc::prelude::*;

use crate::rpc::cluster::SendEndSignal; // new code

#[derive(Clone, Debug, Deserialize, Serialize)]

pub struct SyncRollupMetadata {
    pub rollup_id: RollupId,

    pub batch_number: u64,
    pub transaction_order: u64,

    pub provided_batch_number: u64,
    pub provided_transaction_order: i64,

    pub provided_epoch: i64,
    pub completed_batch_number: i64,
}

impl RpcParameter<AppState> for SyncRollupMetadata {
    type Response = ();

    fn method() -> &'static str {
        "sync_rollup_metadata"
    }

    async fn handler(self, context: AppState) -> Result<Self::Response, RpcError> {
        println!("===== 🔄🔥🔄🔥🔄 SyncRollupMetadata handler() 시작 🔄🔥🔄🔥🔄 ====="); // test code

        let mut mut_rollup_metadata = RollupMetadata::get_mut(&self.rollup_id)?;

        // 🔥🔥🔥🔥🔥 mut_rollup_metadata synchronization start(SyncLeaderTxOrderer) 🔥🔥🔥🔥🔥
        // 📌 batch_number ✅
        // 📌 transaction_order ✅
        // 📌 provided_batch_number ✅
        // 📌 provided_transaction_order ✅
        // 📌 provided_epoch ✅
        // 📌 completed_batch_number ✅

        // === test code start ===
        println!("  = 🔥🔥🔥🔥🔥 mut_rollup_metadata initialization before update 🔥🔥🔥🔥🔥 ="); // test code
        println!("  mut_rollup_metadata.batch_number: {:?}", mut_rollup_metadata.batch_number); // test code
        println!("  mut_rollup_metadata.transaction_order: {:?}", mut_rollup_metadata.transaction_order); // test code
        println!("  mut_rollup_metadata.provided_batch_number: {:?}", mut_rollup_metadata.provided_batch_number); // test code
        println!("  mut_rollup_metadata.provided_transaction_order: {:?}", mut_rollup_metadata.provided_transaction_order); // test code
        println!("  mut_rollup_metadata.provided_epoch: {:?}", mut_rollup_metadata.provided_epoch); // test code
        println!("  mut_rollup_metadata.completed_batch_number: {:?}", mut_rollup_metadata.completed_batch_number); // test code
        println!("  = 🔥🔥🔥🔥🔥 mut_rollup_metadata initialization after update 🔥🔥🔥🔥🔥 ="); // test code
        // === test code end === 
        
        mut_rollup_metadata.batch_number = self.batch_number; // 🚩 batch_number 
        mut_rollup_metadata.transaction_order = self.transaction_order; // 🚩 transaction_order 
        mut_rollup_metadata.provided_batch_number = self.provided_batch_number; // 🚩 provided_batch_number 
        mut_rollup_metadata.provided_transaction_order = self.provided_transaction_order; // 🚩 provided_transaction_order 

        mut_rollup_metadata.provided_epoch = self.provided_epoch; // new code -> 🚩 provided_epoch 
        mut_rollup_metadata.completed_batch_number = self.completed_batch_number; // new code -> 🚩 completed_batch_number 

        // === test code start ===
        println!("  = 🔥🔥🔥🔥🔥 mut_rollup_metadata initialization before update 🔥🔥🔥🔥🔥 ="); // test code
        println!("  mut_rollup_metadata.batch_number: {:?}", mut_rollup_metadata.batch_number); // test code
        println!("  mut_rollup_metadata.transaction_order: {:?}", mut_rollup_metadata.transaction_order); // test code
        println!("  mut_rollup_metadata.provided_batch_number: {:?}", mut_rollup_metadata.provided_batch_number); // test code
        println!("  mut_rollup_metadata.provided_transaction_order: {:?}", mut_rollup_metadata.provided_transaction_order); // test code
        println!("  mut_rollup_metadata.provided_epoch: {:?}", mut_rollup_metadata.provided_epoch); // test code
        println!("  mut_rollup_metadata.completed_batch_number: {:?}", mut_rollup_metadata.completed_batch_number); // test code
        println!("  = 🔥🔥🔥🔥🔥 mut_rollup_metadata initialization after update 🔥🔥🔥🔥🔥 ="); // test code
        // === test code end === 

        mut_rollup_metadata.update()?;

        Ok(())
    }
}
*/
