use crate::{
    rpc::{
        cluster::{BatchCreationMessage, SyncBatchCreation, SyncRawTransaction},
        external::issue_order_commitment,
        prelude::*,
    },
    task::finalize_batch,
    types::*,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendRawTransaction {
    pub rollup_id: RollupId,
    pub raw_transaction: RawTransaction,
}

impl RpcParameter<AppState> for SendRawTransaction {
    type Response = OrderCommitment;

    fn method() -> &'static str {
        "send_raw_transaction"
    }
    async fn handler(mut self, context: AppState) -> Result<Self::Response, RpcError> { // new code
    // async fn handler(self, context: AppState) -> Result<Self::Response, RpcError> { // old code

        // === test code begin ===
        // println!("=== SendRawTransaction 시작 ===");
        // tracing::info!("=== SendRawTransaction 시작 ===");
        // println!("Rollup ID: {}", self.rollup_id);
        // println!("Transaction Type: {:?}", self.raw_transaction);
        // === test code end ===

        let rollup = Rollup::get(&self.rollup_id)?;

        // println!("Rollup Max Transaction Count Per Batch: {}", mut_rollup_metadata.max_transaction_count_per_batch); // test code

        let cluster_metadata = ClusterMetadata::get(
            rollup.platform,
            rollup.liveness_service_provider,
            &rollup.cluster_id,
        )
        .map_err(|error| {
            tracing::error!("Failed to get cluster metadata: {:?}", error);
            Error::ClusterMetadataNotFound
        })?;

        // === new code start ===
        match &mut self.raw_transaction {
            RawTransaction::Eth(eth_tx) => {
                if eth_tx.epoch.is_none() && eth_tx.current_leader_tx_orderer_address.is_none() { // if the transaction is from the client
                    // set the epoch
                    eth_tx.set_epoch(cluster_metadata.epoch.unwrap_or(0)); 
                    
                    // set the current leader tx orderer address in the raw transaction
                    eth_tx.current_leader_tx_orderer_address = Some(cluster_metadata.leader_tx_orderer_rpc_info.clone().unwrap().tx_orderer_address.as_hex_string());
                    
                    // println!("Enhanced transaction - Epoch: {:?}, Leader: {:?}", eth_tx.epoch, eth_tx.current_leader_tx_orderer_address); // test code
                }
                else {
                    // Transaction already has epoch/leader set (not from client)
                }
            }
            RawTransaction::EthBundle(_) => {}
        }

        // println!("cluster_metadata.epoch: {:?}", cluster_metadata.epoch); // test code

        let signer = context.get_signer(rollup.platform).await.map_err(|_| {
            tracing::error!("Signer not found for platform {:?}", rollup.platform);
            Error::SignerNotFound
        })?;

        // println!("signer.address(): {:?}", signer.address()); // test code

        let tx_orderer_address = signer.address().clone().as_hex_string(); // address of the current tx orderer
        
        // get the current leader tx orderer address from the raw transaction
        let raw_transaction_current_leader_tx_orderer_address = match &self.raw_transaction { 
            RawTransaction::Eth(eth_tx) => eth_tx.current_leader_tx_orderer_address.clone(),
            RawTransaction::EthBundle(_) => None,
        };

        let mut is_current_leader = false;
        if raw_transaction_current_leader_tx_orderer_address.is_some() {
            if raw_transaction_current_leader_tx_orderer_address.clone().unwrap() == tx_orderer_address {
                is_current_leader = true;
            } else {
                is_current_leader = false;
            }
        }
        else {
            // println!("⚠️raw_transaction_current_leader_tx_orderer_address is None"); // test code
            is_current_leader = cluster_metadata.is_leader;
        }

        // println!("raw_transaction_current_leader_tx_orderer_address: {:?}", raw_transaction_current_leader_tx_orderer_address.clone().unwrap()); // test code
        // println!("tx_orderer_address: {:?}", tx_orderer_address); // test code
        // println!("is_current_leader: {:?}", is_current_leader); // test code

        let cluster = Cluster::get(
                rollup.platform,
                rollup.liveness_service_provider,
                &rollup.cluster_id,
                cluster_metadata.platform_block_height,
            )
            .map_err(|error| {
                tracing::error!("Failed to get cluster: {:?}", error);
                Error::ClusterNotFound
            })?;
        // === new code end ===

        if is_current_leader && cluster_metadata.can_process_as_leader {
            // === Leader with processing authority: order immediately ===

            let start_drain_pending = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_nanos();

            // First, drain any pending transactions that were queued before authority was granted
            let pending_txs = PendingRawTransactionModel::try_drain_all(&self.rollup_id)
                .unwrap_or_default();
            for pending_tx in pending_txs {
                process_single_transaction(
                    &context,
                    &self.rollup_id,
                    &rollup,
                    &cluster,
                    pending_tx,
                )
                .await?;
            }

            let end_drain_pending = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_nanos();
            tracing::info!(
                "Drain pending transactions took {} ns ({} ms)",
                end_drain_pending - start_drain_pending,
                (end_drain_pending - start_drain_pending) / 1_000_000
            );

            let mut mut_rollup_metadata = RollupMetadata::get_mut(&self.rollup_id)?;
            // Now process the current transaction
            let batch_number = mut_rollup_metadata.batch_number;
            let transaction_order = mut_rollup_metadata.transaction_order;
            let transaction_hash = self.raw_transaction.raw_transaction_hash();

            mut_rollup_metadata.transaction_order += 1;

            let is_updated = mut_rollup_metadata.check_and_update_batch_info();
            mut_rollup_metadata.update()?;

            RawTransactionModel::put_with_transaction_hash(
                &self.rollup_id,
                &transaction_hash,
                self.raw_transaction.clone(),
                true,
            )?;

            RawTransactionModel::put(
                &self.rollup_id,
                batch_number,
                transaction_order,
                self.raw_transaction.clone(),
                true,
            )?;

            let merkle_tree = context.merkle_tree_manager().get(&self.rollup_id).await?;
            let (_, pre_merkle_path) = merkle_tree.add_data(transaction_hash.as_ref()).await;
            drop(merkle_tree);

            CanProvideTransactionInfo::add_can_provide_transaction_orders(
                &self.rollup_id,
                batch_number,
                vec![transaction_order],
            )?;

            if is_updated {
                context
                    .merkle_tree_manager()
                    .insert(&self.rollup_id, MerkleTree::new())
                    .await;

                finalize_batch(context.clone(), &self.rollup_id, batch_number);
            }

            let order_commitment = issue_order_commitment(
                context.clone(),
                rollup.platform,
                self.rollup_id.clone(),
                rollup.order_commitment_type,
                transaction_hash.clone(),
                batch_number,
                transaction_order,
                pre_merkle_path,
            )
            .await?;

            order_commitment.put(&self.rollup_id, batch_number, transaction_order)?;

            sync_raw_transaction(
                context.clone(),
                cluster,
                self.rollup_id,
                batch_number,
                transaction_order,
                self.raw_transaction.clone(),
                order_commitment.clone(),
                true,
            );

            let builder_rpc_url = context.config().builder_rpc_url.clone();
            let cloned_rpc_client = context.rpc_client();

            if builder_rpc_url.is_some() {
                match self.raw_transaction.clone() {
                    RawTransaction::Eth(eth_raw_transaction) => {
                        let params = serde_json::json!([
                            eth_raw_transaction.raw_transaction,
                            batch_number,
                            transaction_order
                        ]);

                        let _transaction_hash: String = cloned_rpc_client
                            .request(
                                &builder_rpc_url.unwrap(),
                                "eth_sendRawTransaction",
                                &params,
                                Id::Null,
                            )
                            .await
                            .map_err(|error| {
                                tracing::error!("Failed to send raw transaction: {:?}", error);
                                Error::RpcClient(error)
                            })?;
                    }
                    RawTransaction::EthBundle(_eth_bundle_raw_transaction) => {
                        unimplemented!("EthBundle raw transaction is not supported yet");
                    }
                }
            }

            match rollup.order_commitment_type {
                OrderCommitmentType::TransactionHash => Ok(OrderCommitment::Single(
                    SingleOrderCommitment::TransactionHash(TransactionHashOrderCommitment::new(
                        transaction_hash.as_string(),
                    )),
                )),
                OrderCommitmentType::Sign => Ok(order_commitment),
            }
        } else if is_current_leader && !cluster_metadata.can_process_as_leader {
            // === Leader but no processing authority yet: queue the transaction ===

            let pending_index = PendingRawTransactionModel::enqueue(
                &self.rollup_id,
                self.raw_transaction.clone(),
            )?;

            tracing::info!(
                "Transaction queued as pending (index: {}) - leader not yet authorized to process, rollup_id: {}",
                pending_index,
                self.rollup_id,
            );

            let transaction_hash = self.raw_transaction.raw_transaction_hash();
            Ok(OrderCommitment::Single(
                SingleOrderCommitment::TransactionHash(TransactionHashOrderCommitment::new(
                    transaction_hash.as_string(),
                )),
            ))
        } else {
            // === Not the leader: forward to the leader node ===

            let leader_tx_orderer_rpc_info = raw_transaction_current_leader_tx_orderer_address
                .as_ref()
                .and_then(|s| {
                    radius_sdk::signature::Address::from_str(rollup.platform.into(), s).ok()
                })
                .and_then(|addr| cluster.get_tx_orderer_rpc_info(&addr))
                .or_else(|| cluster_metadata.leader_tx_orderer_rpc_info.clone());

            match leader_tx_orderer_rpc_info {
                Some(leader_tx_orderer_rpc_info) => {
                    let leader_external_rpc_url = leader_tx_orderer_rpc_info
                        .external_rpc_url
                        .clone()
                        .ok_or(Error::EmptyLeaderClusterRpcUrl)?;

                    match context
                        .rpc_client()
                        .request(
                            leader_external_rpc_url,
                            SendRawTransaction::method(),
                            &self,
                            Id::Null,
                        )
                        .await
                    {
                        Ok(response) => Ok(response),
                        Err(error) => {
                            tracing::error!(
                                "Send raw transaction - leader external rpc error: {:?}",
                                error
                            );
                            Err(error.into())
                        }
                    }
                }
                None => {
                    tracing::error!("Send raw transaction - leader tx orderer rpc info is None");
                    return Err(Error::EmptyLeader)?;
                }
            }
        }
    }
}

async fn process_single_transaction(
    context: &AppState,
    rollup_id: &RollupId,
    rollup: &Rollup,
    cluster: &Cluster,
    raw_transaction: RawTransaction,
) -> Result<(), RpcError> {
    let mut mut_rollup_metadata = RollupMetadata::get_mut(rollup_id)?;

    let batch_number = mut_rollup_metadata.batch_number;
    let transaction_order = mut_rollup_metadata.transaction_order;
    let transaction_hash = raw_transaction.raw_transaction_hash();

    mut_rollup_metadata.transaction_order += 1;
    let is_updated = mut_rollup_metadata.check_and_update_batch_info();

    mut_rollup_metadata.update()?;

    RawTransactionModel::put_with_transaction_hash(
        rollup_id,
        &transaction_hash,
        raw_transaction.clone(),
        true,
    )?;

    RawTransactionModel::put(
        rollup_id,
        batch_number,
        transaction_order,
        raw_transaction.clone(),
        true,
    )?;

    let merkle_tree = context.merkle_tree_manager().get(rollup_id).await?;
    let (_, _pre_merkle_path) = merkle_tree.add_data(transaction_hash.as_ref()).await;
    drop(merkle_tree);

    CanProvideTransactionInfo::add_can_provide_transaction_orders(
        rollup_id,
        batch_number,
        vec![transaction_order],
    )?;

    if is_updated {
        context
            .merkle_tree_manager()
            .insert(rollup_id, MerkleTree::new())
            .await;

        finalize_batch(context.clone(), rollup_id, batch_number);
    }

    let order_commitment = issue_order_commitment(
        context.clone(),
        rollup.platform,
        rollup_id.clone(),
        rollup.order_commitment_type,
        transaction_hash.clone(),
        batch_number,
        transaction_order,
        _pre_merkle_path,
    )
    .await?;

    order_commitment.put(rollup_id, batch_number, transaction_order)?;

    sync_raw_transaction(
        context.clone(),
        cluster.clone(),
        rollup_id.clone(),
        batch_number,
        transaction_order,
        raw_transaction.clone(),
        order_commitment,
        true,
    );

    Ok(())
}

/*
// 이건 왜 만든거?
pub async fn process_pending_transactions(
    context: &AppState,
    rollup_id: &RollupId,
) -> Result<(), RpcError> {
    let rollup = Rollup::get(rollup_id)?;

    let cluster_metadata = ClusterMetadata::get(
        rollup.platform,
        rollup.liveness_service_provider,
        &rollup.cluster_id,
    )
    .map_err(|error| {
        tracing::error!("Failed to get cluster metadata: {:?}", error);
        Error::ClusterMetadataNotFound
    })?;

    let cluster = Cluster::get(
        rollup.platform,
        rollup.liveness_service_provider,
        &rollup.cluster_id,
        cluster_metadata.platform_block_height,
    )
    .map_err(|error| {
        tracing::error!("Failed to get cluster: {:?}", error);
        Error::ClusterNotFound
    })?;

    let pending_txs = PendingRawTransactionModel::drain_all(rollup_id)
        .unwrap_or_default();

    tracing::info!(
        "Processing {} pending transactions for rollup_id: {}",
        pending_txs.len(),
        rollup_id,
    );

    for pending_tx in pending_txs {
        process_single_transaction(
            context,
            rollup_id,
            &rollup,
            &cluster,
            pending_tx,
        )
        .await?;
    }

    Ok(())
}
*/

#[allow(clippy::too_many_arguments)]
pub fn sync_raw_transaction(
    context: AppState,
    cluster: Cluster,
    rollup_id: RollupId,
    batch_number: u64,
    transaction_order: u64,
    raw_transaction: RawTransaction,
    order_commitment: OrderCommitment,
    is_direct_sent: bool,
) {
    // println!("=== Sync Raw Transaction 시작 ===");

    tokio::spawn(async move {
        let other_cluster_rpc_url_list = cluster.get_other_cluster_rpc_url_list();
        if other_cluster_rpc_url_list.is_empty() {
            return;
        }

        let sync_raw_transaction = SyncRawTransaction {
            rollup_id,
            batch_number,
            transaction_order,
            raw_transaction,
            order_commitment: order_commitment,
            is_direct_sent,
        };

        // println!("=== Sync Raw Transaction Info ===");
        // println!("Batch Number: {}", sync_raw_transaction.batch_number);
        // println!("Transaction Order: {}", sync_raw_transaction.transaction_order);

        context
            .rpc_client()
            .fire_and_forget_multicast(
                other_cluster_rpc_url_list,
                SyncRawTransaction::method(),
                &sync_raw_transaction,
                Id::Null,
            )
            .await;

        // println!("=== Sync Raw Transaction 종료 ===");
    });
}

#[allow(clippy::too_many_arguments)]
pub fn sync_batch_creation(
    context: AppState,
    cluster: Cluster,
    platform: Platform,
    rollup_id: RollupId,
    batch_number: u64,
    batch_commitment: [u8; 32],
    batch_creator_signature: Signature,
) {
    tracing::info!("sync_batch_creation() 시작"); // test code

    tokio::spawn(async move {
        tracing::info!(
            "Sync batch creation - rollup_id: {:?} / batch_number: {:?}",
            rollup_id,
            batch_number
        );

        tracing::info!("sync_batch_creation() - 1"); // test code

        let other_cluster_rpc_url_list = cluster.get_other_cluster_rpc_url_list();
        if other_cluster_rpc_url_list.is_empty() {
            return;
        }

        // === test code start ===
        // Log which signer/address will be used for leader_tx_orderer_signature
        match context.get_signer(platform).await {
            Ok(signer) => {
                tracing::info!(
                    "sync_batch_creation signer address - platform: {:?}, address: {:?}",
                    platform,
                    signer.address()
                );
            }
            Err(e) => {
                tracing::error!(
                    "sync_batch_creation failed to get signer for logging - platform: {:?}, error: {}",
                    platform,
                    e
                );
            }
        }
        // === test code end ===

        let batch_creation_massage = BatchCreationMessage {
            rollup_id: rollup_id.clone(),
            batch_number,
            batch_commitment,
            batch_creator_signature,
        };
        let leader_tx_orderer_signature = match context
            .get_signer(platform)
            .await
            .map_err(|e| tracing::error!("Failed to get signer: {}", e))
            .and_then(|signer| {
                signer
                    .sign_message(&batch_creation_massage)
                    .map_err(|e| tracing::error!("Failed to sign message: {}", e))
            }) {
            Ok(signature) => signature,
            Err(_) => return,
        };

        tracing::info!("sync_batch_creation() - 2"); // test code

        let sync_batch_creation = SyncBatchCreation {
            batch_creation_massage,
            leader_tx_orderer_signature,
        };

        context
            .rpc_client()
            .fire_and_forget_multicast(
                other_cluster_rpc_url_list.clone(),
                SyncBatchCreation::method(),
                &sync_batch_creation,
                Id::Null,
            )
            .await
    });
}
