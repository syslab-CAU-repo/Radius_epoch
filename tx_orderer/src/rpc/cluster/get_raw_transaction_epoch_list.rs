use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// use super::{SyncRollupMetadata}; // new code // 03.05 수정사항: sync_rollup_metadata 요청은 더이상 쓰이지 않으므로 주석 처리함
use crate::rpc::prelude::*;

use super::LeaderChangeMessage; // new code
use crate::rpc::{cluster::sync_leader_tx_orderer, prelude::*};

use radius_sdk::{json_rpc::client::Priority, signature::Address};
use tokio::{sync::mpsc::UnboundedReceiver, time::Instant};

use super::SyncLeaderTxOrderer;
use crate::{
    rpc::{
        cluster::{GetOrderCommitmentInfo, GetOrderCommitmentInfoResponse},
        prelude::*,
    },
    task::{send_transaction_list_to_mev_searcher, MevTargetTransaction},
};

use super::send_end_signal_to_epoch_leader; // new code
use super::get_last_valid_completed_epoch; // new code
use super::my_extract_raw_transactions; // new code
use super::my_extract_raw_transactions_with_meta; // new code
use super::get_last_valid_transaction_order; // new code
use super::my_fetch_and_append_transactions; // new code
use super::my_fetch_and_append_transactions_with_meta; // new code

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GetRawTransactionEpochList {
    pub rollup_id: RollupId,
    pub rollup_signature: Signature,
    pub leader_change_message: LeaderChangeMessage,
}

// === test code start ===
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RawTransactionMeta {
    pub epoch: Option<i64>,
    pub batch_number: u64,
    pub transaction_order: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GetRawTransactionEpochListResponse {
    //  raw_transaction_list: Vec<String>,
    pub raw_transaction_meta_list: Vec<RawTransactionMeta>, // test code
}
// === test code end ===

fn mark_batch_completed(
    batch_number: u64,
    max_contiguous: &mut i64,
    out_of_order_set: &mut BTreeSet<u64>,
) {
    // 아직 어떤 배치도 완료되지 않은 상태(max_contiguous < 0)인 경우를 먼저 처리
    if *max_contiguous < 0 {
        if batch_number == 0 {
            // 첫 번째로 0번 배치가 완료된 경우, 0부터 시작해서 연속 구간을 계산
            let mut new_mc: u64 = 0;
            loop {
                let next = new_mc + 1;
                if out_of_order_set.remove(&next) {
                    new_mc = next;
                } else {
                    break;
                }
            }
            *max_contiguous = new_mc as i64;
        } else {
            // 아직 0번이 끝나지 않았는데 더 큰 번호가 먼저 끝난 경우 → 보류
            out_of_order_set.insert(batch_number);
        }
        return;
    }

    // max_contiguous >= 0 인 일반 케이스
    let mc_u64 = *max_contiguous as u64;

    if batch_number <= mc_u64 {
        // 이미 연속 완료 구간 안이므로 할 일 없음
        return;
    }

    if batch_number == mc_u64 + 1 {
        // 바로 다음 배치가 완료된 경우: 연속 구간을 앞으로 밀어올릴 수 있음
        let mut new_mc = batch_number;

        // out_of_order_set에 “그 다음 번호들”이 들어 있다면
        // 연속으로 이어지는 동안 계속 올려준다.
        loop {
            let next = new_mc + 1;
            if out_of_order_set.remove(&next) {
                new_mc = next;
            } else {
                break;
            }
        }

        *max_contiguous = new_mc as i64;
    } else {
        // 그 외: 앞에 구멍이 있으므로 일단 보류
        out_of_order_set.insert(batch_number);
    }
}

impl RpcParameter<AppState> for GetRawTransactionEpochList {
    type Response = GetRawTransactionEpochListResponse;

    fn method() -> &'static str {
        "get_raw_transaction_epoch_list"
    }

    async fn handler(self, context: AppState) -> Result<Self::Response, RpcError> {
        tracing::info!("===== 🗂️🗂️🗂️🗂️🗂️ GetRawTransactionEpochList handler() 시작 🗂️🗂️🗂️🗂️🗂️ ====="); // test code

        let start_get_raw_transaction_epoch_list_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();

        let mut raw_transaction_epoch_list = Vec::new();
        let mut raw_transaction_meta_list = Vec::new(); // test code

        let rollup_id = self.rollup_id.clone();

        let rollup_metadata = match RollupMetadata::get(&rollup_id) {
            Ok(metadata) => metadata,
            Err(err) => {
                tracing::error!(
                    "Failed to get rollup metadata - rollup_id: {:?} / error: {:?}",
                    rollup_id,
                    err,
                );

                return Ok(GetRawTransactionEpochListResponse {
                    // raw_transaction_list: Vec::new(),
                    raw_transaction_meta_list: Vec::new(), // test code
                });
            }
        };

        let rollup = Rollup::get(&rollup_id)?;

        // 현재까지 완료된(leader node가 end_signal을 받고 CanProvideEpochInfo에 추가한) 가장 최신의 epoch를 받아옴(CanProvideEpochInfo에서 받아옴)
        let latest_completed_epoch = match CanProvideEpochInfo::get(&rollup_id) {
            Ok(can_provide_epoch) => {
                let completed_epoch_list = &can_provide_epoch.completed_epoch;
                // println!("🔍 completed_epoch_list: {:?}", completed_epoch_list); // test code
                match get_last_valid_completed_epoch(
                    completed_epoch_list,
                    rollup_metadata.provided_epoch,
                ) {
                    Ok(last_valid_epoch) => last_valid_epoch,
                    Err(err) => {
                        tracing::error!(
                            "Failed to get epoch - rollup_id: {:?} / error: {:?}",
                            rollup_id,
                            err
                        );
                        return Ok(GetRawTransactionEpochListResponse {
                            // raw_transaction_list: Vec::new(),
                            raw_transaction_meta_list: Vec::new(),
                        });
                    }
                }
            }
            Err(err) => {
                tracing::error!(
                    "Failed to get can_provide_epoch - rollup_id: {:?} / error: {:?}",
                    rollup_id,
                    err
                );
                return Ok(GetRawTransactionEpochListResponse {
                    // raw_transaction_list: Vec::new(),
                    raw_transaction_meta_list: Vec::new(),
                });
            }
        };
        tracing::info!("💡latest_completed_epoch(CanProvideEpochInfo에서 받아온 값): {:?}", latest_completed_epoch); // test code

        let provided_epoch = rollup_metadata.provided_epoch; // 저번 get 요청에서 처리된 epoch 최댓값(이 epoch 이하는 다시 볼 필요 없음)
        tracing::info!("💡provided_epoch(RollupMetadata에서 받아온 값): {:?}", provided_epoch); // test code

        let max_contiguous = rollup_metadata.max_contiguous; // 저번 get 요청에서 처리된 가장 최신의 batch 번호들 중 앞에 구멍이 없는 최댓값
        let mut current_completed_batch_number = max_contiguous; // rollup_metadata.max_contiguous 갱신을 위한 mut 변수
        let mut current_provided_batch_number = max_contiguous + 1; // 현재 처리 시작할 batch 번호

        tracing::info!("current_completed_batch_number(Batch 순회 전): {:?}", current_completed_batch_number); // test code
        tracing::info!("current_provided_batch_number(Batch 순회 전): {:?}", current_provided_batch_number); // test code
        
        let mut iteration_count = 0; // test code

        let mut mut_rollup_metadata = RollupMetadata::get_mut(&rollup_id)?;

        let mut get_succeeded_batch = false;

        while let Ok(batch) = Batch::get(&rollup_id, current_provided_batch_number as u64) { // current_provided_batch_number is i64, but Batch::get requires u64. This variable is always a non-negative integer so this won't cause an error.
            tracing::info!("= {:?}th batch interation(Batch 번호: {:?}) =", iteration_count, current_provided_batch_number); // test code

            let mut pending_uncompleted_epoch_count = 0;
            let extracted = my_extract_raw_transactions_with_meta(
                batch,
                latest_completed_epoch,
                provided_epoch,
                current_provided_batch_number as u64,
                &mut pending_uncompleted_epoch_count,
            );
            for (raw_transaction, meta) in extracted {
                raw_transaction_epoch_list.push(raw_transaction);
                raw_transaction_meta_list.push(meta);
            }

            if pending_uncompleted_epoch_count == 0 {
                // 해당 배치의 모든 트랜잭션이 처리 완료된 경우, 연속 완료 구간을 안전하게 갱신
                mark_batch_completed(
                    current_provided_batch_number as u64,
                    &mut current_completed_batch_number,
                    &mut mut_rollup_metadata.out_of_order_completed_batches,
                );
            }
            
            current_provided_batch_number += 1; 

            get_succeeded_batch = true;

            iteration_count += 1; // test code
        }

        let current_provided_transaction_order = mut_rollup_metadata.provided_transaction_order; // (02.05 수정사항) CanProvideTransactionInfo 지난 요청에서 어디까지 진행됐는지 받아옴
        tracing::info!("💡current_provided_transaction_order(RollupMetadata에서 받아온 값): {:?}", current_provided_transaction_order); // test code

        tracing::info!("current_completed_batch_number(Batch 순회 후): {:?}", current_completed_batch_number); // test code
        tracing::info!("current_provided_batch_number(Batch 순회 후): {:?}", current_provided_batch_number); // test code
        // println!("current_provided_transaction_order(Batch 순회 후): {:?}", current_provided_transaction_order); // test code

        let mut get_succeeded_can_provide_transaction_info = false;

        if let Ok(can_provide_transaction_info) = CanProvideTransactionInfo::get(&rollup_id) {
            if let Some(can_provide_transaction_orderers) = can_provide_transaction_info
                .can_provide_transaction_orders_per_batch
                .get(&(current_provided_batch_number as u64))
            {
                let valid_end_transaction_order = get_last_valid_transaction_order(
                    can_provide_transaction_orderers,
                    current_provided_transaction_order,
                );
                tracing::info!("💡valid_end_transaction_order(get_last_valid_transaction_order()에서 받아온 값): {:?}", valid_end_transaction_order); // test code
        
                my_fetch_and_append_transactions_with_meta(
                    &rollup_id,
                    current_provided_batch_number as u64,
                    0,
                    valid_end_transaction_order,
                    &mut raw_transaction_epoch_list,
                    &mut raw_transaction_meta_list,
                    &latest_completed_epoch,
                    provided_epoch,
                )?;

                get_succeeded_can_provide_transaction_info = true;

                /*
                if current_provided_transaction_order
                    == rollup.max_transaction_count_per_batch as i64 - 1
                {
                    // current_provided_batch_number += 1; // (02.17 수정사항) current_provided_batch_number 갱신 로직 주석 처리 
                    current_provided_transaction_order = -1;
                }
                */
            } else {
                tracing::info!(
                    "CanProvideTransactionInfo는 있었지만 해당 batch에 대한 transaction order 정보가 없습니다. rollup_id: {:?}, batch_number: {:?}",
                    rollup_id,
                    current_provided_batch_number
                );
            }
        } else {
            tracing::info!(
                "CanProvideTransactionInfo::get(&rollup_id)에 실패했습니다. rollup_id: {:?}",
                rollup_id
            );
        }

        let cluster = Cluster::get(
            rollup.platform,
            rollup.liveness_service_provider,
            &rollup.cluster_id,
            self.leader_change_message.platform_block_height,
        )?;

        /* // 03.05 수정사항: batch_number_list_to_delete는 사용하지 않는 코드이므로 주석 처리함
        let mut batch_number_list_to_delete = Vec::new();
        for batch_number in (last_completed_batch_number + 1)..current_completed_batch_number {
            batch_number_list_to_delete.push(batch_number);
        }
        */

        mut_rollup_metadata.provided_batch_number = current_provided_batch_number as u64;
        mut_rollup_metadata.provided_transaction_order = current_provided_transaction_order; // (02.05 수정사항) CanProvideTransactionInfo 이번 요청에서 어디까지 진행됐는지 저장

        mut_rollup_metadata.max_contiguous = current_completed_batch_number; // new code

        if get_succeeded_batch && get_succeeded_can_provide_transaction_info {
            mut_rollup_metadata.provided_epoch = latest_completed_epoch as i64; // new code
        } else {
            tracing::info!("provided_epoch를 갱신하지 않습니다. 기존 provided_epoch: {:?}, latest_completed_epoch: {:?}", mut_rollup_metadata.provided_epoch, latest_completed_epoch);
        }

        let leader_tx_orderer_rpc_info = cluster
            .get_tx_orderer_rpc_info(&self.leader_change_message.next_leader_tx_orderer_address)
            .ok_or_else(|| {
                tracing::error!(
                    "TxOrderer RPC info not found for address {:?}",
                    self.leader_change_message.next_leader_tx_orderer_address
                );
                Error::TxOrdererInfoNotFound
            })?;

        let signer = context.get_signer(rollup.platform).await.map_err(|_| {
            tracing::error!("Signer not found for platform {:?}", rollup.platform);
            Error::SignerNotFound
        })?;

        let tx_orderer_address = signer.address().clone();

        let is_next_leader =
            tx_orderer_address == self.leader_change_message.next_leader_tx_orderer_address;

        let mut mut_cluster_metadata = ClusterMetadata::get_mut(
            rollup.platform,
            rollup.liveness_service_provider,
            &rollup.cluster_id,
        )?;

        /*
        // ??? 이건 original 코드에는 없는 내용
        let cluster_metadata = ClusterMetadata::get(
            rollup.platform,
            rollup.liveness_service_provider,
            &rollup.cluster_id,
        )
        .map_err(|error| {
            tracing::error!("Failed to get cluster metadata: {:?}", error);
            Error::ClusterMetadataNotFound
        })?;
        // ???

        */

        if mut_cluster_metadata.is_leader == false {
            // tracing::info!("*** if mut_cluster_metadata.is_leader == false ***"); // test code

            if let Some(current_leader_tx_orderer_rpc_info) =
                mut_cluster_metadata.leader_tx_orderer_rpc_info.clone()
            {
                let current_leader_tx_orderer_cluster_rpc_url = current_leader_tx_orderer_rpc_info
                    .cluster_rpc_url
                    .clone()
                    .unwrap();

                // tracing::info!("current_leader_tx_orderer_cluster_rpc_url: {:?}", current_leader_tx_orderer_cluster_rpc_url); // test code
                // tracing::info!("current_leader_tx_orderer_rpc_info: {:?}", current_leader_tx_orderer_rpc_info); // test code

                let parameter = GetOrderCommitmentInfo {
                    rollup_id: self.rollup_id.clone(),
                };

                match context
                    .rpc_client()
                    .request_with_priority::<&GetOrderCommitmentInfo, GetOrderCommitmentInfoResponse>(
                        current_leader_tx_orderer_cluster_rpc_url.clone(),
                        GetOrderCommitmentInfo::method(),
                        &parameter,
                        Id::Null,
                        Priority::High,
                    )
                    .await
                {
                    Ok(response) => {
                        tracing::info!(
                          "Get order commitment info - current leader external rpc response: {:?}",
                          response
                        );

                        mut_rollup_metadata.batch_number = response.batch_number;
                        mut_rollup_metadata.transaction_order = response.transaction_order;
                    }
                    Err(error) => {
                        tracing::error!(
                            "Get order commitment info - current leader external rpc error: {:?}",
                            error
                        );
                    }
                }
            } else {
                tracing::warn!(
                    "Current leader tx orderer RPC info not found for address {:?}",
                    self.leader_change_message.current_leader_tx_orderer_address
                );
            }
        }

        mut_cluster_metadata.platform_block_height =
            self.leader_change_message.platform_block_height; // 🚩 platform_block_height 
        mut_cluster_metadata.is_leader = is_next_leader; // 🚩 is_leader 
        mut_cluster_metadata.leader_tx_orderer_rpc_info = Some(leader_tx_orderer_rpc_info.clone()); // 🚩 leader_tx_orderer_rpc_info 

        // === new code start ===
        let old_epoch = if let Some(epoch) = mut_cluster_metadata.epoch {
            epoch
        } else {
            tracing::error!("Cannot assign an old epoch — the epoch in ClusterMetadata is missing for some reason.");
            return Ok(GetRawTransactionEpochListResponse {
                // raw_transaction_list: Vec::new(),
                raw_transaction_meta_list: Vec::new(),
            });
        };

        // old_epoch의 리더 RPC URL을 epoch_leader_map에 저장 (이미 존재하지 않을 때만)
        if !mut_cluster_metadata.epoch_leader_map.contains_key(&old_epoch) {
            tracing::info!("old_epoch의 리더 RPC URL을 epoch_leader_map에 저장 (이미 존재하지 않을 때만)"); // test code
            if let Some(current_leader_rpc_info) = cluster.get_tx_orderer_rpc_info(&self.leader_change_message.current_leader_tx_orderer_address) {
                if let Some(cluster_rpc_url) = &current_leader_rpc_info.cluster_rpc_url {
                    mut_cluster_metadata.epoch_leader_map.insert(old_epoch, cluster_rpc_url.clone());
                    tracing::info!("old_epoch의 리더 RPC URL을 epoch_leader_map에 저장 완료"); // test code
                }
            }
        }

        mut_cluster_metadata.epoch = Some(old_epoch + 1); // 🚩 epoch 

        let new_epoch = if let Some(epoch) = mut_cluster_metadata.epoch {
            epoch
        } else {
            tracing::error!("Cannot assign an old epoch — the epoch in ClusterMetadata is missing for some reason.");
            return Ok(GetRawTransactionEpochListResponse {
                // raw_transaction_list: Vec::new(),
                raw_transaction_meta_list: Vec::new(),
            });
        };

        // new_epoch의 리더 RPC URL을 epoch_leader_map에 저장
        if let Some(cluster_rpc_url) = &leader_tx_orderer_rpc_info.cluster_rpc_url {
            mut_cluster_metadata.epoch_leader_map.insert(new_epoch, cluster_rpc_url.clone()); // 🚩 epoch_leader_map
        } else {
            tracing::error!(
                "cluster_rpc_url not found for new leader - rollup_id: {:?}, new_epoch: {}",
                self.leader_change_message.rollup_id,
                new_epoch
            );
        }
        // === new code end ===

        // old_epoch의 리더 RPC URL 가져오기
        let epoch_leader_rpc_url = mut_cluster_metadata.epoch_leader_map.get(&old_epoch).cloned().ok_or_else(|| {
            tracing::error!(
                "epoch_leader_rpc_url not found for old_epoch: {:?} - rollup_id: {:?}, cluster_id: {:?}",
                old_epoch,
                self.leader_change_message.rollup_id,
                rollup.cluster_id
            );
            Error::GeneralError("epoch_leader_rpc_url not found".into())
        })?;

        /* // ??? 이거 들어가야함???
        let signer = context.get_signer(rollup.platform).await?;
        let current_tx_orderer_address = signer.address();
        */

        sync_leader_tx_orderer(
            context.clone(),
            cluster,
            // current_tx_orderer_address, // old code(not used)
            self.leader_change_message.clone(),
            self.rollup_signature,
            mut_rollup_metadata.batch_number,
            mut_rollup_metadata.transaction_order,
            mut_rollup_metadata.provided_batch_number,
            mut_rollup_metadata.provided_transaction_order,
            mut_rollup_metadata.provided_epoch,
            mut_rollup_metadata.max_contiguous,
            mut_rollup_metadata.out_of_order_completed_batches.clone(),
            &self.leader_change_message.current_leader_tx_orderer_address,
            old_epoch,
            new_epoch,
            epoch_leader_rpc_url.clone(), // 사용 안 함
        )
        .await;

        /*
        // ??? --> 이거 get_raw_transaction_epoch_list 따로 디커플링했을 때 추가한 코드 같은데 sync_leader_tx_orderer 함수에서 처리되는 거니까 이거 필요없음???

        if let Err(error) = sync_rollup_metadata(
            context.clone(),
            rollup_id.clone(),
            mut_rollup_metadata.batch_number,
            mut_rollup_metadata.transaction_order,
            mut_rollup_metadata.provided_batch_number,
            mut_rollup_metadata.provided_transaction_order,
            mut_rollup_metadata.provided_epoch,
            mut_rollup_metadata.completed_batch_number,
        )
        .await
        {
            tracing::error!(
                "sync_rollup_metadata error - rollup id: {:?}, error: {:?}",
                rollup_id,
                error
            );
        }

        */

        mut_cluster_metadata.update()?;

        let _ = mut_rollup_metadata.update().map_err(|error| {
            tracing::error!(
                "rollup_metadata update error - rollup id: {:?}, error: {:?}",
                rollup_id,
                error
            );
        });

        send_end_signal_to_epoch_leader(
            context.clone(),
            self.leader_change_message.rollup_id.clone(),
            old_epoch,
            epoch_leader_rpc_url,
        );

        let end_get_raw_transaction_epoch_list_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();

        tracing::info!(
            "get_raw_transaction_epoch_list - total take time: {:?}",
            end_get_raw_transaction_epoch_list_time - start_get_raw_transaction_epoch_list_time
        );

        let shared_channel_infos = context.shared_channel_infos();
        let mev_searcher_infos = MevSearcherInfos::get_or(MevSearcherInfos::default).unwrap();

        send_transaction_list_to_mev_searcher(
            &rollup_id,
            raw_transaction_epoch_list.clone(),
            shared_channel_infos,
            &mev_searcher_infos,
        );

        let ip_list = mev_searcher_infos.get_ip_list_by_rollup_id(&rollup_id);
        let receivers: Vec<Arc<tokio::sync::Mutex<UnboundedReceiver<MevTargetTransaction>>>> = {
            let map = shared_channel_infos.lock().unwrap();
            ip_list
                .iter()
                .filter_map(|ip| map.get(ip).map(|(_, rx)| Arc::clone(rx)))
                .collect()
        };

        let collected_mev_target_transaction = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let mut sub_tasks = vec![];

        for receiver in receivers {
            let collected_clone = Arc::clone(&collected_mev_target_transaction);
            let rx = Arc::clone(&receiver);

            let sub_task = tokio::spawn(async move {
                let deadline = Instant::now() + Duration::from_millis(5000);

                tokio::select! {
                    _ = tokio::time::sleep_until(deadline) => {}
                    maybe_mev_target_transaction = async {
                        let mut guard = rx.lock().await;
                        guard.recv().await
                    } => {
                        if let Some(mev_target_transaction) = maybe_mev_target_transaction {
                            tracing::info!("Received mev target transaction: {:?}", mev_target_transaction);
                            collected_clone.lock().await.push(mev_target_transaction);
                        }
                    }
                }
            });

            sub_tasks.push(sub_task);
        }

        let _ = futures::future::join_all(sub_tasks).await;

        {
            let result = collected_mev_target_transaction.lock().await;
            tracing::info!("Collected mev target transactions: {:?}", *result);

            for mev_target_transaction in result.iter() {
                raw_transaction_epoch_list
                    .extend(mev_target_transaction.backrunning_transaction_list.clone());
            }
        }

        tracing::info!("===== 🗂️🗂️🗂️🗂️🗂️ GetRawTransactionEpochList handler() 종료(노드 주소: {:?}, raw_transaction_meta_list 길이: {}) 🗂️🗂️🗂️🗂️🗂️ =====", tx_orderer_address, raw_transaction_meta_list.len()); // test code

        Ok(GetRawTransactionEpochListResponse {
            // raw_transaction_list: raw_transaction_epoch_list,
            raw_transaction_meta_list,
        })
    }
}

/*
// not used
pub async fn sync_rollup_metadata(
    context: AppState,
    rollup_id: RollupId,
    batch_number: u64,
    transaction_order: u64,
    provided_batch_number: u64,
    provided_transaction_order: i64,
    provided_epoch: i64, 
    completed_batch_number: i64, 
) -> Result<(), radius_sdk::kvstore::KvStoreError> {
    tracing::info!("=== 🔄🔥 sync_rollup_metadata 시작 🔥🔄 ==="); // test code

    let rollup = Rollup::get(&rollup_id)?;

    let cluster_metadata = ClusterMetadata::get(
        rollup.platform,
        rollup.liveness_service_provider,
        &rollup.cluster_id,
    )?;

    let cluster = Cluster::get(
        rollup.platform,
        rollup.liveness_service_provider,
        &rollup.cluster_id,
        cluster_metadata.platform_block_height,
    )?;

    let other_cluster_rpc_url_list = cluster.get_other_cluster_rpc_url_list();
    if other_cluster_rpc_url_list.is_empty() {
        tracing::info!("No cluster RPC URLs available for synchronization");
        return Ok(());
    }

    let parameter = SyncRollupMetadata {
        rollup_id: rollup_id.clone(),
        batch_number,
        transaction_order,
        provided_batch_number,
        provided_transaction_order,
        provided_epoch,
        completed_batch_number,
    };

    let urls = other_cluster_rpc_url_list.clone();

    let context_clone = context.clone();

    tokio::spawn(async move {
        let _ = context_clone
            .rpc_client()
            .fire_and_forget_multicast(
                urls,
                SyncRollupMetadata::method(),
                &parameter,
                Id::Null,
            )
            .await;
    });

    Ok(())
}
*/