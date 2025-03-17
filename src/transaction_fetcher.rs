use crate::config::Config;
use crate::models::Transaction;
use crate::repositories::transaction_repository;
use chrono::{DateTime, Utc};
use log::{info, warn};
use mongodb::Database;
use near_jsonrpc_client::{methods, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_primitives::types::{BlockReference, FunctionArgs};
use num_bigint::BigInt;
use serde_json::Value;
use std::error::Error;
use std::str::FromStr;
use tokio::time::{sleep, Duration};

pub async fn fetch_and_process_transactions(
    config: &Config,
    db: &Database,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<Vec<Transaction>, Box<dyn Error>> {
    // TODO uncomment this
    let last_transaction = transaction_repository::get_latest_transaction(db).await?;
    let last_block_height = last_transaction.map(|t| t.block_height).unwrap_or(0); // Default to 0 if no transactions exist

    info!(
        "Fetching transactions from block height: {}",
        last_block_height
    );
    let transactions =
        fetch_new_transactions(&config.validator_account_id, last_block_height).await?;

    info!("Fetched {} raw transactions", transactions.len());

    let processed_transactions =
        process_transactions(transactions, config, primary_client, secondary_client).await?;

    info!("Processed {} transactions", processed_transactions.len());

    if !processed_transactions.is_empty() {
        transaction_repository::save_transactions(db, &processed_transactions).await?;
        info!(
            "Saved {} new transactions to the database",
            processed_transactions.len()
        );
    } else {
        info!("No new transactions to save");
    }

    Ok(processed_transactions)
}

async fn fetch_new_transactions(
    validator_account: &str,
    last_block_height: u64,
) -> Result<Vec<Value>, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let mut all_transactions = Vec::new();
    let mut current_page = 1;
    let per_page = 25;
    let max_retries = 5;

    'outer: loop {
        let url = format!(
            "https://api.nearblocks.io/v1/account/{}/stake-txns?per_page={}&order=asc&page={}&after_block={}",
            validator_account, per_page, current_page, last_block_height
        );

        for attempt in 0..max_retries {
            info!(
                "Fetching transactions from URL: {} (Attempt {})",
                url,
                attempt + 1
            );

            let response = client.get(&url).send().await?;

            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                warn!("Rate limit reached. Waiting for 60 seconds before retrying...");
                sleep(Duration::from_secs(60)).await;
                continue;
            }

            let json: Value = response.json().await?;

            if let Some(error_message) = json.get("message") {
                if error_message
                    .as_str()
                    .unwrap_or("")
                    .contains("exceeded your API request limit")
                {
                    warn!("Rate limit reached. Waiting for 60 seconds before retrying...");
                    sleep(Duration::from_secs(60)).await;
                    continue;
                }
            }

            if let Some(txns) = json.get("txns").and_then(|v| v.as_array()) {
                if txns.is_empty() {
                    info!("No more transactions to fetch");
                    break 'outer;
                }
                info!(
                    "Fetched {} transactions on page {}",
                    txns.len(),
                    current_page
                );
                all_transactions.extend(txns.clone());
                current_page += 1;
                break;
            } else {
                warn!("Unexpected response format from API: {:?}", json);
                if attempt == max_retries - 1 {
                    return Err("Max retries reached with unexpected response format".into());
                }
                sleep(Duration::from_secs(60)).await;
            }
        }
    }

    info!("Total transactions fetched: {}", all_transactions.len());
    Ok(all_transactions)
}

fn safe_parse_amount(amount_str: &str) -> Result<String, Box<dyn Error>> {
    let cleaned_str = amount_str
        .trim()
        .trim_matches('"')
        .split('.')
        .next()
        .unwrap_or("0")
        .to_string();

    BigInt::from_str(&cleaned_str)
        .map(|n| n.to_string())
        .map_err(|e| Box::new(e) as Box<dyn Error>)
}

async fn process_transactions(
    transactions: Vec<Value>,
    config: &Config,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<Vec<Transaction>, Box<dyn Error>> {
    let mut processed_transactions = Vec::new();

    for tx in transactions {
        if let Some(result) =
            analyze_staking_transaction(&tx, config, primary_client, secondary_client).await?
        {
            processed_transactions.push(result);
        }
    }

    Ok(processed_transactions)
}

async fn analyze_staking_transaction(
    tx: &Value,
    config: &Config,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<Option<Transaction>, Box<dyn Error>> {
    let tx_hash = tx["transaction_hash"].as_str().unwrap_or_default();
    let tx_data = get_transaction_receipts(tx_hash, primary_client, secondary_client).await?;

    if let Some(result) =
        analyze_receipts(&tx_data, tx, config, primary_client, secondary_client).await?
    {
        let type_ = determine_type(&result.action, &result.method);
        let block_height = tx["block"]["block_height"].as_u64().unwrap_or_default();
        let timestamp = tx["block_timestamp"].as_str().unwrap_or_default();
        let delegator_address = tx["predecessor_account_id"].as_str().unwrap_or_default();

        let timestamp_nanos = timestamp.parse::<i64>()?;
        let datetime = DateTime::<Utc>::from_timestamp(timestamp_nanos / 1_000_000_000, 0)
            .unwrap_or_else(|| Utc::now());

        let amount = safe_parse_amount(&result.amount)?;

        Ok(Some(Transaction {
            transaction_hash: tx_hash.to_string(),
            amount,
            method: result.method,
            action: result.action,
            type_: type_,
            block_height,
            timestamp: datetime,
            delegator_address: delegator_address.to_string(),
        }))
    } else {
        Ok(None)
    }
}

async fn get_transaction_receipts(
    transaction_hash: &str,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<Value, Box<dyn Error>> {
    let request = methods::EXPERIMENTAL_tx_status::RpcTransactionStatusRequest {
        transaction_info: methods::EXPERIMENTAL_tx_status::TransactionInfo::TransactionId {
            hash: near_primitives::hash::CryptoHash::from_str(transaction_hash)
                .map_err(|e| Box::<dyn Error>::from(e.to_string()))?,
            account_id: near_primitives::types::AccountId::from_str("system")
                .map_err(|e| Box::<dyn Error>::from(e.to_string()))?,
        },
    };

    let response = match primary_client.call(&request).await {
        Ok(response) => response,
        Err(_) => secondary_client.call(&request).await?,
    };

    Ok(serde_json::to_value(response)?)
}

async fn analyze_receipts(
    tx_data: &Value,
    tx: &Value,
    config: &Config,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<Option<StakingAction>, Box<dyn Error>> {
    let mut total_stake_amount = BigInt::from(0);
    let mut total_unstake_amount = BigInt::from(0);
    let mut action = None;

    if let Some(receipts) = tx_data["receipts_outcome"].as_array() {
        for receipt in receipts {
            if let Some(result) =
                analyze_receipt(receipt, tx, config, primary_client, secondary_client).await?
            {
                match result.action.as_str() {
                    "stake" => {
                        let amount = safe_parse_amount(&result.amount)?;
                        total_stake_amount += BigInt::from_str(&amount)?;
                        action = Some("stake".to_string());
                    }
                    "unstake" => {
                        let amount = safe_parse_amount(&result.amount)?;
                        total_unstake_amount += BigInt::from_str(&amount)?;
                        action = Some("unstake".to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    if action.is_none() {
        let method = tx["actions"][0]["method"].as_str().unwrap_or_default();
        if method == "deposit_and_stake" {
            action = Some("stake".to_string());
            let amount = tx["actions_agg"]["deposit"].as_str().unwrap_or("0");
            total_stake_amount = BigInt::from_str(&safe_parse_amount(amount)?)?;
        }
    }

    if let Some(action) = action {
        Ok(Some(StakingAction {
            action: action.clone(),
            amount: if action == "stake" {
                total_stake_amount.to_string()
            } else {
                total_unstake_amount.to_string()
            },
            method: tx["actions"][0]["method"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
        }))
    } else {
        let deposit = tx["actions_agg"]["deposit"].as_str().unwrap_or("0");
        Ok(Some(StakingAction {
            action: "stake".to_string(),
            amount: safe_parse_amount(deposit)?,
            method: tx["actions"][0]["method"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
        }))
    }
}

async fn analyze_receipt(
    receipt: &Value,
    transaction: &Value,
    config: &Config,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<Option<StakingAction>, Box<dyn Error>> {
    if let Some(logs) = receipt["outcome"]["logs"].as_array() {
        for log in logs {
            if let Some(staking_action) = parse_staking_log(log.as_str().unwrap_or_default()) {
                return Ok(Some(staking_action));
            }
        }
    }

    if let Some(actions) = receipt["receipt"]["Action"]["actions"].as_array() {
        for action in actions {
            if let Some(function_call) = action.get("FunctionCall") {
                if let Some(result) = analyze_function_call(
                    function_call,
                    transaction,
                    config,
                    primary_client,
                    secondary_client,
                )
                .await?
                {
                    return Ok(Some(result));
                }
            }
        }
    }

    Ok(None)
}

fn parse_staking_log(log: &str) -> Option<StakingAction> {
    if log.contains(r#""event":"dist.stak""#) {
        if let Ok(json_log) = serde_json::from_str::<Value>(log) {
            return Some(StakingAction {
                action: "stake".to_string(),
                amount: json_log["amount"].as_str().unwrap_or("0").to_string(),
                method: "distribute_staking".to_string(),
            });
        }
    }

    let staking_keywords = [
        ("deposited", "stake"),
        ("staking", "stake"),
        ("unstaking", "unstake"),
        ("withdrew", "unstake"),
    ];

    for (keyword, action) in &staking_keywords {
        if log.contains(keyword) {
            if let Some(amount) = log
                .split_whitespace()
                .find(|&part| part.parse::<f64>().is_ok())
            {
                return Some(StakingAction {
                    action: action.to_string(),
                    amount: amount.to_string(),
                    method: "unknown".to_string(),
                });
            }
        }
    }

    None
}

async fn analyze_function_call(
    function_call: &Value,
    transaction: &Value,
    config: &Config,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<Option<StakingAction>, Box<dyn Error>> {
    let staking_methods = [
        ("deposit_and_stake", "stake"),
        ("stake", "stake"),
        ("unstake", "unstake"),
        ("unstake_all", "unstake"),
        ("withdraw", "unstake"),
        ("withdraw_all", "unstake"),
        ("distribute_staking", "stake"),
    ];

    let method = function_call["method_name"].as_str().unwrap_or_default();

    for &(method_name, action) in &staking_methods {
        if method == method_name {
            let amount = if method == "unstake" || method == "unstake_all" {
                get_unstake_amount(
                    transaction,
                    function_call,
                    config,
                    primary_client,
                    secondary_client,
                )
                .await?
            } else if method.contains("all") {
                "all".to_string()
            } else {
                function_call["deposit"]
                    .as_str()
                    .or_else(|| transaction["actions_agg"]["deposit"].as_str())
                    .unwrap_or("0")
                    .to_string()
            };

            return Ok(Some(StakingAction {
                action: action.to_string(),
                amount,
                method: method.to_string(),
            }));
        }
    }

    Ok(None)
}

async fn get_unstake_amount(
    transaction: &Value,
    function_call: &Value,
    config: &Config,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<String, Box<dyn Error>> {
    let account_id = transaction["signer_id"].as_str().unwrap_or_default();
    let block_height = transaction["block_height"].as_u64().unwrap_or_default();

    let prev_block_balance = get_account_stake_balance(
        config,
        primary_client,
        secondary_client,
        account_id,
        block_height - 1,
    )
    .await?;

    if function_call["method_name"].as_str().unwrap_or_default() == "unstake_all" {
        Ok(prev_block_balance)
    } else {
        let args = function_call["args"].as_str().unwrap_or("{}");
        let args: Value = serde_json::from_str(args)?;
        let amount = args["amount"]
            .as_str()
            .or_else(|| function_call["deposit"].as_str())
            .or_else(|| transaction["actions_agg"]["deposit"].as_str())
            .unwrap_or("0");
        safe_parse_amount(amount)
    }
}

async fn get_account_stake_balance(
    config: &Config,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    account_id: &str,
    block_height: u64,
) -> Result<String, Box<dyn Error>> {
    let query_request = methods::query::RpcQueryRequest {
        block_reference: BlockReference::BlockId(near_primitives::types::BlockId::Height(
            block_height,
        )),
        request: near_primitives::views::QueryRequest::CallFunction {
            account_id: config.validator_account_id.parse()?,
            method_name: "get_account".to_string(),
            args: FunctionArgs::from(
                serde_json::json!({ "account_id": account_id })
                    .to_string()
                    .into_bytes(),
            ),
        },
    };

    let result = match primary_client.call(&query_request).await {
        Ok(response) => response,
        Err(_) => secondary_client.call(&query_request).await?,
    };

    if let QueryResponseKind::CallResult(call_result) = result.kind {
        let account_info: Value = serde_json::from_slice(&call_result.result)?;
        let staked_balance = account_info["staked_balance"].as_str().unwrap_or("0");
        safe_parse_amount(staked_balance)
    } else {
        Ok("0".to_string())
    }
}

fn determine_type(action: &str, method: &str) -> String {
    match action {
        "unstake" => "unstake".to_string(),
        "stake" => "stake".to_string(),
        _ => match method {
            "deposit_and_stake" | "stake" | "distribute_staking" => "stake".to_string(),
            "unstake" | "unstake_all" | "withdraw" | "withdraw_all" => "unstake".to_string(),
            _ => {
                eprintln!(
                    "Unexpected action/method combination: {}/{}",
                    action, method
                );
                "stake".to_string()
            }
        },
    }
}

#[derive(Debug)]
struct StakingAction {
    action: String,
    amount: String,
    method: String,
}
