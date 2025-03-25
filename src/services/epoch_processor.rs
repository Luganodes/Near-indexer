use crate::config::Config;
use crate::models::{DelegatorData, Transaction};
use crate::repositories::{delegator_repository, epoch_repository, validator_repository};
use crate::services::near_rpc;
use log::{info, warn};
use mongodb::Database;
use near_jsonrpc_client::JsonRpcClient;
use num_bigint::BigInt;
use num_traits::Zero;
use std::collections::HashMap;
use std::str::FromStr;

const EPOCHS_PER_YEAR: f64 = 730.0; // 365 days * 2 epochs per day

fn calculate_rewards(
    current_stake: &str,
    previous_stake: Option<&String>,
    transaction_total: Option<&BigInt>,
) -> String {
    let current = BigInt::from_str(current_stake).unwrap_or_else(|_| BigInt::zero());
    let previous = previous_stake
        .and_then(|s| BigInt::from_str(s).ok())
        .unwrap_or_else(|| BigInt::zero());
    let tx_total = transaction_total.cloned().unwrap_or_else(|| BigInt::zero());

    // For first epoch with no previous stake
    if previous.is_zero() && !current.is_zero() {
        return "0".to_string(); // First stake is not a reward
    }

    // Clone the values before the arithmetic operations
    let current_clone = current.clone();
    let previous_clone = previous.clone();
    let tx_total_clone = tx_total.clone();

    // If there's a transaction, it will already be reflected in current_stake
    // So we just need to subtract previous stake
    let rewards = current - (previous + tx_total);

    if rewards < BigInt::zero() {
        warn!(
            "Negative rewards calculated: {} = {} - ({} + {})",
            rewards, current_clone, previous_clone, tx_total_clone
        );
        "0".to_string()
    } else {
        rewards.to_string()
    }
}

fn calculate_apy(rewards: &str, stake_amount: &str) -> f64 {
    let rewards_big = BigInt::from_str(rewards).unwrap_or_else(|_| BigInt::zero());
    let stake_big = BigInt::from_str(stake_amount).unwrap_or_else(|_| BigInt::zero());

    if stake_big.is_zero() {
        return 0.0;
    }

    // Debug logging
    info!(
        "Calculating APY - Rewards: {}, Stake: {}",
        rewards_big, stake_big
    );

    // Convert to f64, handling the yoctoNEAR conversion implicitly
    // We'll keep the numbers in yoctoNEAR to maintain precision
    let rewards_f64 = rewards_big.to_string().parse::<f64>().unwrap_or(0.0);
    let stake_f64 = stake_big.to_string().parse::<f64>().unwrap_or(1.0);

    // Calculate epoch rate
    let epoch_rate = rewards_f64 / stake_f64;

    // Annualize the rate
    let annual_rate = epoch_rate * EPOCHS_PER_YEAR;

    // Convert to percentage and round to 2 decimal places
    let apy = (annual_rate * 100.0).round() / 100.0;

    // Debug logging
    info!(
        "APY Calculation - Epoch Rate: {}, Annual Rate: {}, Final APY: {}%",
        epoch_rate, annual_rate, apy
    );

    apy
}

fn calculate_initial_stakes(transactions: &[&Transaction]) -> HashMap<String, BigInt> {
    let mut stakes = HashMap::new();

    let mut sorted_transactions = transactions.to_vec();
    sorted_transactions.sort_by_key(|tx| tx.block_height);

    for tx in sorted_transactions {
        let delegator = &tx.delegator_address;
        let amount = match BigInt::from_str(&tx.amount) {
            Ok(value) => value,
            Err(e) => {
                warn!(
                    "Failed to parse amount {} for transaction {}: {}",
                    tx.amount, tx.transaction_hash, e
                );
                continue;
            }
        };

        let stake = stakes
            .entry(delegator.clone())
            .or_insert_with(|| BigInt::zero());

        match tx.type_.as_str() {
            "stake" => *stake += &amount,
            "unstake" => *stake -= &amount,
            _ => {
                warn!(
                    "Unknown transaction type {} for transaction {}",
                    tx.type_, tx.transaction_hash
                );
            }
        }
    }

    for (delegator, stake) in stakes.iter() {
        info!("Final stake for delegator {}: {}", delegator, stake);
    }

    stakes
}

fn calculate_epoch_transaction_totals(transactions: &[&Transaction]) -> HashMap<String, BigInt> {
    let mut totals = HashMap::new();

    for tx in transactions {
        let delegator = &tx.delegator_address;
        let amount = BigInt::from_str(&tx.amount).unwrap_or_else(|_| BigInt::zero());

        let total = totals
            .entry(delegator.clone())
            .or_insert_with(|| BigInt::zero());

        match tx.type_.as_str() {
            "stake" => *total += amount,
            "unstake" => *total -= amount,
            _ => {}
        }
    }

    totals
}

pub async fn process_delegator_data(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    validator_account_id: &str,
    start_block_height: u64,
    end_block_height: u64,
    transactions: &[Transaction],
    epoch_number: u64,
    epoch_id: &str,
    epoch_timestamp: u64,
    db: &Database,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("processDelegatorData called with: start_block_height: {}, end_block_height: {}, epoch_number: {}, epoch_id: {}, epoch_timestamp: {}",
          start_block_height, end_block_height, epoch_number, epoch_id, epoch_timestamp);

    let mut delegator_data = HashMap::new();
    let mut total_stake = BigInt::zero();
    let mut total_rewards = BigInt::zero();

    // Get all previous transactions for initial stake calculation
    let all_prev_transactions: Vec<_> = transactions
        .iter()
        .filter(|tx| tx.block_height >= start_block_height && tx.block_height <= end_block_height)
        .collect();

    // Calculate initial stakes from all previous transactions
    let initial_stakes = calculate_initial_stakes(&all_prev_transactions);
    info!(
        "Calculated initial stakes for {} delegators",
        initial_stakes.len()
    );

    // Get previous epoch's stake data
    let prev_epoch_stakes = get_previous_epoch_data(
        primary_client,
        secondary_client,
        validator_account_id,
        start_block_height,
        transactions,
        db,
    )
    .await?;

    // Filter transactions for this specific epoch
    let epoch_transactions: Vec<_> = transactions
        .iter()
        .filter(|tx| tx.block_height >= start_block_height && tx.block_height <= end_block_height)
        .collect();

    info!(
        "Found {} transactions for current epoch",
        epoch_transactions.len()
    );

    let epoch_transaction_totals = calculate_epoch_transaction_totals(&epoch_transactions);

    // Process accounts and calculate rewards/APY
    let accounts = match near_rpc::get_accounts(
        primary_client,
        secondary_client,
        validator_account_id,
        start_block_height,
    )
    .await
    {
        Ok(accounts) => accounts,
        Err(e) => return Err(e.into()),
    };

    for account in accounts {
        let account_id = account["account_id"].as_str().unwrap().to_string();
        let staked_balance = account["staked_balance"].as_str().unwrap().to_string();

        let initial_stake = initial_stakes
            .get(&account_id)
            .cloned()
            .unwrap_or_else(|| BigInt::zero())
            .to_string();

        let rewards = calculate_rewards(
            &staked_balance,
            prev_epoch_stakes.get(&account_id),
            epoch_transaction_totals.get(&account_id),
        );

        let apy = calculate_apy(&rewards, &staked_balance);

        total_stake += BigInt::from_str(&staked_balance).unwrap_or_else(|_| BigInt::zero());
        total_rewards += BigInt::from_str(&rewards).unwrap_or_else(|_| BigInt::zero());

        delegator_data.insert(
            account_id.clone(),
            DelegatorData {
                delegator_id: account_id.clone(),
                validator_account_id: validator_account_id.to_string(),
                epoch: epoch_number,
                start_block_height,
                end_block_height,
                timestamp: epoch_timestamp,
                initial_stake,
                auto_compounded_stake: staked_balance,
                last_update_block: start_block_height,
                epoch_id: epoch_id.to_string(),
                rewards,
                apy,
            },
        );
    }

    // Calculate validator-wide APY
    let validator_apy = calculate_apy(&total_rewards.to_string(), &total_stake.to_string());

    // Save all data
    epoch_repository::save_epoch_data(
        db,
        epoch_number,
        epoch_id,
        &delegator_data,
        validator_account_id,
        start_block_height,
        end_block_height,
        &epoch_transactions,
        epoch_timestamp,
    )
    .await?;

    validator_repository::save_validator_metrics(
        db,
        validator_account_id,
        epoch_number,
        epoch_id,
        &delegator_data,
        epoch_timestamp,
        validator_apy,
    )
    .await?;

    let delegator_data_vec: Vec<DelegatorData> = delegator_data.values().cloned().collect();
    delegator_repository::save_delegator_data(db, &delegator_data_vec, config.delegator_batch_size)
        .await?;

    info!(
        "Processed epoch {} (ID: {}). Validator APY: {}%",
        epoch_number, epoch_id, validator_apy
    );

    Ok(())
}

async fn get_previous_epoch_data(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    validator_account_id: &str,
    current_start_block: u64,
    transactions: &[Transaction],
    _db: &Database,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    // Find the first transaction before the current epoch start
    let prev_block = transactions
        .iter()
        .filter(|tx| tx.block_height < current_start_block)
        .map(|tx| tx.block_height)
        .max()
        .unwrap_or(0);

    if prev_block == 0 {
        return Ok(HashMap::new());
    }

    let accounts = near_rpc::get_accounts(
        primary_client,
        secondary_client,
        validator_account_id,
        prev_block,
    )
    .await?;

    let mut prev_stakes = HashMap::new();
    for account in accounts {
        let account_id = account["account_id"].as_str().unwrap().to_string();
        let staked_balance = account["staked_balance"].as_str().unwrap().to_string();
        prev_stakes.insert(account_id, staked_balance);
    }

    Ok(prev_stakes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_calculate_rewards() {
        // Test case 1: Normal reward calculation
        assert_eq!(
            calculate_rewards(
                "26000008342448094319999999",
                Some(&"26000000000000000000000000".to_string()),
                Some(&BigInt::zero())
            ),
            "8342448094319999999"
        );

        // Test case 2: First epoch (no previous stake)
        assert_eq!(
            calculate_rewards(
                "26000000000000000000000000",
                None,
                Some(&BigInt::from_str("26000000000000000000000000").unwrap())
            ),
            "0"
        );

        // Test case 3: With transaction
        assert_eq!(
            calculate_rewards(
                "26100008342448094319999999",
                Some(&"26000000000000000000000000".to_string()),
                Some(&BigInt::from_str("100000000000000000000000").unwrap())
            ),
            "8342448094319999999"
        );
    }

    #[test]
    fn test_calculate_apy() {
        // Test case 1: ~0.1% reward per epoch
        assert_eq!(
            calculate_apy("26000000000000000000000", "26000000000000000000000000"),
            20.79
        );

        // Test case 2: No rewards
        assert_eq!(calculate_apy("0", "26000000000000000000000000"), 0.0);

        // Test case 3: No stake
        assert_eq!(calculate_apy("1000000000000000000000", "0"), 0.0);
    }

    #[test]
    fn test_calculate_initial_stakes() {
        let now = Utc::now();
        let transactions = vec![
            Transaction {
                transaction_hash: "hash1".to_string(),
                amount: "100".to_string(),
                method: "deposit_and_stake".to_string(),
                action: "stake".to_string(),
                type_: "stake".to_string(),
                block_height: 1,
                timestamp: now,
                delegator_address: "user1".to_string(),
            },
            Transaction {
                transaction_hash: "hash2".to_string(),
                amount: "50".to_string(),
                method: "unstake".to_string(),
                action: "unstake".to_string(),
                type_: "unstake".to_string(),
                block_height: 2,
                timestamp: now,
                delegator_address: "user1".to_string(),
            },
        ];

        let transaction_refs: Vec<&Transaction> = transactions.iter().collect();
        let initial_stakes = calculate_initial_stakes(&transaction_refs);

        assert_eq!(
            initial_stakes.get("user1").unwrap().to_string(),
            "50",
            "Initial stake should be 100 - 50 = 50"
        );
    }
}
