use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegatorData {
    pub delegator_id: String,
    pub validator_account_id: String,
    pub epoch: u64,
    pub start_block_height: u64,
    pub end_block_height: u64,
    pub timestamp: u64,
    pub initial_stake: String,
    pub auto_compounded_stake: String,
    pub last_update_block: u64,
    pub epoch_id: String,
    pub rewards: String,
    pub apy: String, // New field for APY
}
