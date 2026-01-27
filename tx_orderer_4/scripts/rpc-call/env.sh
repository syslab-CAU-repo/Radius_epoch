# tx_orderer/scripts/rpc-call/env.sh

#!/bin/bash
TX_ORDERER_INTERNAL_RPC_URL="http://165.194.35.14:11101" # This should be matched with TX_ORDERER_INTERNAL_RPC_URL in tx_orderer/scripts/execute/env.sh

################################# Sequencing (liveness) Contract ####################
LIVENESS_PLATFORM="ethereum"
LIVENESS_SERVICE_PROVIDER="radius"
LIVENESS_RPC_URL="http://165.194.35.15:8545"
LIVENESS_WS_URL="ws://165.194.35.15:8545"
LIVENESS_SERVICE_MANAGER_CONTRACT_ADDRESS="0xbdEd0D2bf404bdcBa897a74E6657f1f12e5C6fb6"
CLUSTER_ID="radius_cluster"

################################# Validation Contract ###############################
### For symbiotic
VALIDATION_PLATFORM="ethereum" # Option: [ethereum]
VALIDATION_SERVICE_PROVIDER="symbiotic" # Option:  [eigen_layer / symbiotic]
VALIDATION_RPC_URL="http://165.194.35.15:8545"
VALIDATION_WS_URL="ws://165.194.35.15:8545"
VALIDATION_SERVICE_MANAGER_CONTRACT_ADDRESS="0x1CfD8455F189c56a4FBd81EB7D4118DB04616BA8"

