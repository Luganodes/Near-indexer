use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction {
    pub transaction_hash: String,
    pub amount: String,
    pub method: String,
    pub action: String,
    pub type_: String,
    pub block_height: u64,
    pub timestamp: DateTime<Utc>,
    pub delegator_address: String,
}
