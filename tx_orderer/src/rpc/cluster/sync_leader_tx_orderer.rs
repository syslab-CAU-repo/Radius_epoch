use std::{
    collections::BTreeSet,
    time::{SystemTime, UNIX_EPOCH},
};

use radius_sdk::json_rpc::server::ProcessPriority;

use super::LeaderChangeMessage;
use crate::rpc::prelude::*;

use crate::rpc::cluster::SendEndSignal; // new code

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncLeaderTxOrderer {
    pub leader_change_message: LeaderChangeMessage,
    pub rollup_signature: Signature,

    pub batch_number: u64,
    pub transaction_order: u64,

    pub provided_batch_number: u64,
    pub provided_transaction_order: i64,

    pub provided_epoch: i64, // new code
    pub max_contiguous: i64, // new code

    pub out_of_order_completed_batches: BTreeSet<u64>, // new code

    pub old_epoch: Option<i64>, // new code
    pub new_epoch: Option<i64>, // new code
}

impl RpcParameter<AppState> for SyncLeaderTxOrderer {
    type Response = ();

    fn method() -> &'static str {
        "sync_leader_tx_orderer"
    }

    fn priority(&self) -> ProcessPriority {
        ProcessPriority::High
    }

    async fn handler(self, context: AppState) -> Result<Self::Response, RpcError> {
        
        tracing::info!("===== 🔄🔄🔄🔄🔄 SyncLeaderTxOrderer handler() 시작 🔄🔄🔄🔄🔄 ====="); // test code

        /*
        println!("self.rollup_signature: {:?}", self.rollup_signature); // test code
        println!("self.batch_number: {:?}", self.batch_number); // test code
        println!("self.transaction_order: {:?}", self.transaction_order); // test code
        println!("self.provided_batch_number: {:?}", self.provided_batch_number); // test code
        println!("self.provided_transaction_order: {:?}", self.provided_transaction_order); // test code
        println!("self.leader_change_message: {:?}", self.leader_change_message); // test code
        println!("self.leader_change_message.platform_block_height: {:?}", self.leader_change_message.platform_block_height); // test code
        println!("self.leader_change_message.current_leader_tx_orderer_address: {:?}", self.leader_change_message.current_leader_tx_orderer_address); // test code
        println!("self.leader_change_message.next_leader_tx_orderer_address: {:?}", self.leader_change_message.next_leader_tx_orderer_address); // test code
        */
        
        let rollup_id = self.leader_change_message.rollup_id.clone();

        let start_sync_leader_tx_orderer_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();

        let rollup = Rollup::get(&rollup_id).map_err(|e| {
            tracing::error!("Failed to retrieve rollup: {:?}", e);
            Error::RollupNotFound
        })?;

        /*
        // === test code start ===
        println!("= rollup initialization ="); // test code
        println!("rollup.platform: {:?}", rollup.platform); // test code
        println!("rollup.liveness_service_provider: {:?}", rollup.liveness_service_provider); // test code
        println!("rollup.cluster_id: {:?}", rollup.cluster_id); // test code
        println!("self.leader_change_message.platform_block_height: {:?}", self.leader_change_message.platform_block_height); // test code
        // === test code end ===
        */

        let cluster = Cluster::get(
            rollup.platform,
            rollup.liveness_service_provider,
            &rollup.cluster_id,
            self.leader_change_message.platform_block_height,
        )?;

        /*
        // === test code start ===
        println!("= cluster initialization ="); // test code
        println!("cluster.tx_orderer_address: {:?}", cluster.tx_orderer_address); // test code
        println!("cluster.rollup_id_list: {:?}", cluster.rollup_id_list); // test code
        println!("cluster.tx_orderer_rpc_infos: {:?}", cluster.tx_orderer_rpc_infos); // test code
        println!("cluster.block_margin: {:?}", cluster.block_margin); // test code
        // === test code end ===
        */

        let signer = context.get_signer(rollup.platform).await.map_err(|_| {
            tracing::error!("Signer not found for platform {:?}", rollup.platform);
            Error::SignerNotFound
        })?;
        let tx_orderer_address = signer.address().clone();
        let is_leader =
            tx_orderer_address == self.leader_change_message.next_leader_tx_orderer_address;

        
        // === test code start ===
        // println!("is_leader: {:?}", is_leader); // test code
        // println!("signer.address() value: {:?}", signer.address()); // test code
        // println!("self.leader_change_message.next_leader_tx_orderer_address: {:?}", self.leader_change_message.next_leader_tx_orderer_address); // test code
        // === test code end ===
        
        let leader_tx_orderer_rpc_info = cluster
            .get_tx_orderer_rpc_info(&self.leader_change_message.next_leader_tx_orderer_address)
            .ok_or_else(|| {
                tracing::error!(
                    "TxOrderer RPC info not found for address {:?}",
                    self.leader_change_message.next_leader_tx_orderer_address
                );
                Error::TxOrdererInfoNotFound
            })?;

        /*
        // === test code start ===
        println!("= leader_tx_orderer_rpc_info initialization ="); // test code
        println!("leader_tx_orderer_rpc_info.cluster_rpc_url: {:?}", leader_tx_orderer_rpc_info.cluster_rpc_url); // test code
        println!("leader_tx_orderer_rpc_info.external_rpc_url: {:?}", leader_tx_orderer_rpc_info.external_rpc_url); // test code
        println!("leader_tx_orderer_rpc_info.tx_orderer_address: {:?}", leader_tx_orderer_rpc_info.tx_orderer_address); // test code
        // === test code end ===
        */
        
        let mut mut_cluster_metadata = ClusterMetadata::get_mut(
            rollup.platform,
            rollup.liveness_service_provider,
            &rollup.cluster_id,
        )?;

        /*
        // === test code start ===
        println!("= mut_cluster_metadata initialization ="); // test code
        println!("mut_cluster_metadata.platform_block_height: {:?}", mut_cluster_metadata.platform_block_height); // test code
        println!("mut_cluster_metadata.is_leader: {:?}", mut_cluster_metadata.is_leader); // test code
        println!("mut_cluster_metadata.leader_tx_orderer_rpc_info: {:?}", mut_cluster_metadata.leader_tx_orderer_rpc_info); // test code
        // === test code end ===
        */
        
        // 🚀🚀🚀🚀🚀 mut_cluster_metadata synchronization start(SyncLeaderTxOrderer) 🚀🚀🚀🚀🚀
        // 📌 platform_block_height ✅
        // 📌 is_leader ✅
        // 📌 leader_tx_orderer_rpc_info ✅
        // 📌 epoch ✅
        // 📌 epoch_leader_map ✅
        // 📌 epoch_node_bitmap -> no need to synchronize

        tracing::info!("🔄🔄 🚀🚀🚀🚀🚀 mut_cluster_metadata before update 🚀🚀🚀🚀🚀"); // test code
        // println!("mut_cluster_metadata.platform_block_height: {:?}", mut_cluster_metadata.platform_block_height); // test code
        // println!("mut_cluster_metadata.is_leader: {:?}", mut_cluster_metadata.is_leader); // test code
        // println!("mut_cluster_metadata.leader_tx_orderer_rpc_info: {:?}", mut_cluster_metadata.leader_tx_orderer_rpc_info); // test code
        tracing::info!("🔄🔄 💡mut_cluster_metadata.epoch(업데이트 전): {:?}", mut_cluster_metadata.epoch); // test code
        // println!("mut_cluster_metadata.epoch_node_bitmap: {:?}", mut_cluster_metadata.epoch_node_bitmap); // test code
        // println!("mut_cluster_metadata.epoch_leader_map: {:?}", mut_cluster_metadata.epoch_leader_map); // test code
        tracing::info!("🔄🔄 🚀🚀🚀🚀🚀 mut_cluster_metadata before update 🚀🚀🚀🚀🚀"); // test code

        mut_cluster_metadata.platform_block_height =
            self.leader_change_message.platform_block_height; // 🚩 platform_block_height 
        mut_cluster_metadata.is_leader = is_leader; // 🚩 is_leader 
        mut_cluster_metadata.leader_tx_orderer_rpc_info = Some(leader_tx_orderer_rpc_info.clone()); // 🚩 leader_tx_orderer_rpc_info 

        // === new code start ===

        // old_epoch가 없으면 오류 출력
        let old_epoch = self.old_epoch.ok_or_else(|| {
            tracing::error!("old_epoch is missing in SyncLeaderTxOrderer request - rollup_id: {:?}", rollup_id);
            Error::GeneralError("old_epoch is missing".into())
        })?;

        // println!("old_epoch: {:?}", old_epoch); // test code

        // old_epoch의 리더 RPC URL을 epoch_leader_map에 저장 (이미 존재하지 않을 때만)
        if !mut_cluster_metadata.epoch_leader_map.contains_key(&old_epoch) {
            tracing::info!("old_epoch의 리더 RPC URL을 epoch_leader_map에 저장 (이미 존재하지 않을 때만)"); // test code
            if let Some(current_leader_rpc_info) = cluster.get_tx_orderer_rpc_info(&self.leader_change_message.current_leader_tx_orderer_address) {
                if let Some(cluster_rpc_url) = &current_leader_rpc_info.cluster_rpc_url {
                    mut_cluster_metadata.epoch_leader_map.insert(old_epoch, cluster_rpc_url.clone());
                }
            }
        }

        // 리더가 바뀌었을 때 SyncLeaderTxOrderer 요청을 받은 노드에서 SyncLeaderTxOrderer 요청에 담긴 new_epoch 값을 사용하여 epoch를 업데이트함(동기화)
        mut_cluster_metadata.epoch = self.new_epoch; // new code -> 🚩 epoch

        // new_epoch의 리더(next_leader) RPC URL도 epoch_leader_map에 저장 (동기화)
        if let Some(new_epoch_value) = self.new_epoch {
            if let Some(cluster_rpc_url) = &leader_tx_orderer_rpc_info.cluster_rpc_url {
                mut_cluster_metadata.epoch_leader_map.insert(new_epoch_value, cluster_rpc_url.clone()); // 🚩 epoch_leader_map
            } else {
                tracing::error!(
                    "cluster_rpc_url not found for new leader - rollup_id: {:?}, new_epoch: {}",
                    rollup_id,
                    new_epoch_value
                );
            }
        }

        // === new code end ===
        // 💫💫💫💫💫 mut_cluster_metadata synchronization end(SyncLeaderTxOrderer) 💫💫💫💫💫

        tracing::info!("🔄🔄 💫💫💫💫💫 mut_cluster_metadata after update 💫💫💫💫💫"); // test code
        // println!("mut_cluster_metadata.platform_block_height: {:?}", mut_cluster_metadata.platform_block_height); // test code
        // println!("mut_cluster_metadata.is_leader: {:?}", mut_cluster_metadata.is_leader); // test code
        // println!("mut_cluster_metadata.leader_tx_orderer_rpc_info: {:?}", mut_cluster_metadata.leader_tx_orderer_rpc_info); // test code
        tracing::info!("🔄🔄 💡mut_cluster_metadata.epoch(업데이트 후): {:?}", mut_cluster_metadata.epoch); // test code
        // println!("mut_cluster_metadata.epoch_node_bitmap: {:?}", mut_cluster_metadata.epoch_node_bitmap); // test code
        // println!("mut_cluster_metadata.epoch_leader_map: {:?}", mut_cluster_metadata.epoch_leader_map); // test code
        tracing::info!("🔄🔄 💫💫💫💫💫 mut_cluster_metadata after update 💫💫💫💫💫"); // test code

        mut_cluster_metadata.update()?;

        let mut mut_rollup_metadata = RollupMetadata::get_mut(&rollup_id)?;

        // 🔥🔥🔥🔥🔥 mut_rollup_metadata synchronization start(SyncLeaderTxOrderer) 🔥🔥🔥🔥🔥
        // 📌 batch_number ✅
        // 📌 transaction_order ✅
        // 📌 provided_batch_number ✅
        // 📌 provided_transaction_order ✅
        // 📌 provided_epoch ✅
        // 📌 completed_batch_number ✅

        // === test code start ===
        // println!("🔄🔄 🔥🔥🔥🔥🔥 mut_rollup_metadata initialization before update 🔥🔥🔥🔥🔥"); // test code
        // println!("🔄🔄 mut_rollup_metadata.batch_number: {:?}", mut_rollup_metadata.batch_number); // test code
        // println!("🔄🔄 mut_rollup_metadata.transaction_order: {:?}", mut_rollup_metadata.transaction_order); // test code
        // println!("🔄🔄 mut_rollup_metadata.provided_batch_number: {:?}", mut_rollup_metadata.provided_batch_number); // test code
        // println!("🔄🔄 mut_rollup_metadata.provided_transaction_order: {:?}", mut_rollup_metadata.provided_transaction_order); // test code
        // println!("🔄🔄 mut_rollup_metadata.provided_epoch: {:?}", mut_rollup_metadata.provided_epoch); // test code
        // println!("🔄🔄 mut_rollup_metadata.completed_batch_number: {:?}", mut_rollup_metadata.completed_batch_number); // test code
        // println!("🔄🔄 🔥🔥🔥🔥🔥 mut_rollup_metadata initialization after update 🔥🔥🔥🔥🔥"); // test code
        // === test code end ===
        
        mut_rollup_metadata.batch_number = self.batch_number; // 🚩 batch_number 
        mut_rollup_metadata.transaction_order = self.transaction_order; // 🚩 transaction_order 
        mut_rollup_metadata.provided_batch_number = self.provided_batch_number; // 🚩 provided_batch_number 
        mut_rollup_metadata.provided_transaction_order = self.provided_transaction_order; // 🚩 provided_transaction_order 

        mut_rollup_metadata.provided_epoch = self.provided_epoch; // new code -> 🚩 provided_epoch 
        mut_rollup_metadata.max_contiguous = self.max_contiguous; // new code -> 🚩 max_contiguous 
        mut_rollup_metadata.out_of_order_completed_batches =
            self.out_of_order_completed_batches.clone(); // new code -> 🚩 out_of_order_completed_batches

        // === test code start ===
        // println!("🔄🔄 🔥🔥🔥🔥🔥 mut_rollup_metadata initialization before update 🔥🔥🔥🔥🔥"); // test code
        // println!("🔄🔄 mut_rollup_metadata.batch_number: {:?}", mut_rollup_metadata.batch_number); // test code
        // println!("🔄🔄 mut_rollup_metadata.transaction_order: {:?}", mut_rollup_metadata.transaction_order); // test code
        // println!("🔄🔄 mut_rollup_metadata.provided_batch_number: {:?}", mut_rollup_metadata.provided_batch_number); // test code
        // println!("🔄🔄 mut_rollup_metadata.provided_transaction_order: {:?}", mut_rollup_metadata.provided_transaction_order); // test code
        // println!("🔄🔄 mut_rollup_metadata.provided_epoch: {:?}", mut_rollup_metadata.provided_epoch); // test code
        // println!("🔄🔄 mut_rollup_metadata.completed_batch_number: {:?}", mut_rollup_metadata.completed_batch_number); // test code
        // println!("🔄🔄 🔥🔥🔥🔥🔥 mut_rollup_metadata initialization after update 🔥🔥🔥🔥🔥"); // test code
        // === test code end === 

        /*
        println!("= mut_rollup_metadata update ="); // test code
        println!("mut_rollup_metadata.batch_number: {:?}", mut_rollup_metadata.batch_number); // test code
        println!("mut_rollup_metadata.transaction_order: {:?}", mut_rollup_metadata.transaction_order); // test code
        println!("mut_rollup_metadata.provided_batch_number: {:?}", mut_rollup_metadata.provided_batch_number); // test code
        println!("mut_rollup_metadata.provided_transaction_order: {:?}", mut_rollup_metadata.provided_transaction_order); // test code
        */
        
        mut_rollup_metadata.update()?;

        // 🔥🔥🔥🔥🔥 mut_rollup_metadata synchronization end(SyncLeaderTxOrderer) 🔥🔥🔥🔥🔥

        // === new code start ===
        let cluster_metadata = ClusterMetadata::get(
            rollup.platform,
            rollup.liveness_service_provider,
            &rollup.cluster_id,
        )?;

        // epoch_leader_rpc_url이 없으면 오류 출력
        let epoch_leader_rpc_url = cluster_metadata.epoch_leader_map.get(&old_epoch).ok_or_else(|| {
            tracing::error!(
                "epoch_leader_rpc_url not found for old_epoch: {:?} - rollup_id: {:?}, cluster_id: {:?}",
                old_epoch,
                rollup_id,
                rollup.cluster_id
            );
            Error::GeneralError("epoch_leader_rpc_url not found".into())
        })?;

        // println!("💡epoch_leader_rpc_url: {:?}", epoch_leader_rpc_url); // test code

        send_end_signal_to_epoch_leader(
            context.clone(),
            rollup_id,
            old_epoch,
            epoch_leader_rpc_url.clone(),
        );
        // === new code end ===

        let end_sync_leader_tx_orderer_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();

        tracing::info!(
            "sync_leader_tx_orderer - total take time: {:?} / self: {:?}",
            end_sync_leader_tx_orderer_time - start_sync_leader_tx_orderer_time,
            self
        );

        tracing::info!("=== 🔄🔄🔄🔄🔄 SyncLeaderTxOrderer handler() 종료(노드 주소: {:?}) 🔄🔄🔄🔄🔄 ===", tx_orderer_address); // test code

        Ok(())
    }
}

// === new code start ===
pub fn send_end_signal_to_epoch_leader(
    context: AppState,
    rollup_id: RollupId,
    epoch: i64,
    epoch_leader_rpc_url: String,
) {
    tracing::info!("=== 📤⚙️ send_end_signal_to_epoch_leader 시작(epoch: {:?}) ⚙️📤 ===", epoch); // test code

    tokio::spawn(async move {
        let rollup = match Rollup::get(&rollup_id) {
            Ok(rollup) => rollup,
            Err(e) => {
                tracing::error!("Failed to retrieve rollup: {:?}", e);
                return;
            }
        };

        let signer = match context.get_signer(rollup.platform).await {
            Ok(signer) => signer,
            Err(e) => {
                tracing::error!("Failed to get signer: {:?}", e);
                return;
            }
        };

        let sender_address = signer.address().clone();
        let sender_address_clone = sender_address.clone();

        tracing::info!("📤⚙️ send_end_signal 요청 보내는 노드: {:?}", sender_address_clone); // test code
        tracing::info!("📤⚙️ epoch: {:?}", epoch); // test code
        tracing::info!("📤⚙️ epoch {:?}의 리더 노드 url: {:?}", epoch, epoch_leader_rpc_url); // test code

        let parameter = SendEndSignal {
            rollup_id,
            epoch,
            sender_address: sender_address_clone,
        };

        context
            .rpc_client()
            .fire_and_forget_multicast(
                vec![epoch_leader_rpc_url],
                SendEndSignal::method(),
                &parameter,
                Id::Null,
            )
            .await;
        
        tracing::info!("=== 📤⚙️ send_end_signal_to_epoch_leader 종료(epoch: {:?}) ⚙️📤 ===", epoch); // test code
    });

    
}
// === new code end ===