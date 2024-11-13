use std::env;
pub struct Config {
    pub validator_account_id: String,
    pub primary_rpc: String,
    pub secondary_rpc: String,
    pub parallel_limit: usize,
    pub batch_size: usize,
    pub epoch_blocks: u64,
    pub delegator_batch_size: usize,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            validator_account_id: env::var("VALIDATOR_ACCOUNT_ID")
                .unwrap_or_else(|_| "luganodes.pool.near".to_string()),
            primary_rpc: env::var("PRIMARY_RPC").expect("PRIMARY_RPC must be set"),
            secondary_rpc: env::var("SECONDARY_RPC").expect("SECONDARY_RPC must be set"),
            parallel_limit: env::var("PARALLEL_LIMIT")
                .unwrap_or_else(|_| "35".to_string())
                .parse()
                .unwrap(),
            batch_size: env::var("BATCH_SIZE")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap(),
            epoch_blocks: env::var("EPOCH_BLOCKS")
                .unwrap_or_else(|_| "43200".to_string())
                .parse()
                .unwrap(),
            delegator_batch_size: env::var("DELEGATOR_BATCH_SIZE")
                .unwrap_or_else(|_| "1000".to_string())
                .parse()
                .unwrap(),
        }
    }
}
