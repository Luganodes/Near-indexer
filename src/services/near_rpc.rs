use crate::models::EpochInfo;
use chrono::{DateTime, Utc};
use log::{error, info, warn};
use near_jsonrpc_client::{methods, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind as JsonRpcQueryResponseKind;
use near_primitives::types::{BlockReference, Finality, FunctionArgs};
use near_primitives::views::BlockView;

// Replace your get_validators_info function with this one
pub async fn get_validators_info(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    epoch_id: Option<&str>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    info!("Fetching validators info for epoch_id: {:?}", epoch_id);

    let params = match epoch_id {
        Some(id) => serde_json::json!([{"epoch_id": id}]),
        None => serde_json::json!([null]),
    };

    // Create a raw JSON-RPC request
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "dontcare",
        "method": "validators",
        "params": params
    });

    // Try primary first, fall back to secondary with retry logic
    let client = reqwest::Client::new();
    let max_retries = 3;
    let mut retry_count = 0;
    let mut backoff_time = 5; // Start with 1 second backoff

    loop {
        info!(
            "Attempting validators API call for epoch {:?}, attempt {} of {}",
            epoch_id,
            retry_count + 1,
            max_retries
        );

        let response = match client
            .post(primary_client.server_addr())
            .json(&request)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    info!("Primary RPC rate limit reached, will try secondary");
                    None
                } else {
                    Some(resp)
                }
            }
            Err(e) => {
                warn!("Error with primary RPC: {}", e);
                None
            }
        };

        if let Some(resp) = response {
            info!("Primary RPC response received");
            let json = resp.json::<serde_json::Value>().await?;
            return Ok(json);
        }

        // Try secondary
        info!("Trying secondary RPC endpoint for validators data");
        match client
            .post(secondary_client.server_addr())
            .json(&request)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    warn!("Secondary RPC rate limit reached, will retry after backoff");
                    // Fall through to retry logic
                } else {
                    info!("Secondary RPC response received");
                    let json = resp.json::<serde_json::Value>().await?;
                    return Ok(json);
                }
            }
            Err(e) => {
                warn!("Error with secondary RPC: {}", e);
                // Fall through to retry logic
            }
        };

        // Both primary and secondary failed, implement backoff and retry
        retry_count += 1;
        if retry_count >= max_retries {
            return Err(format!(
                "Failed to fetch validators info after {} retries",
                max_retries
            )
            .into());
        }

        info!(
            "Both RPCs rate limited, backing off for {} seconds before retry",
            backoff_time
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_time)).await;
        backoff_time *= 2; // Exponential backoff
    }
}

pub async fn create_near_connections(
    primary_rpc: &str,
    secondary_rpc: &str,
) -> (JsonRpcClient, JsonRpcClient) {
    info!("Connecting to NEAR...");
    let primary_client = JsonRpcClient::connect(primary_rpc);
    let secondary_client = JsonRpcClient::connect(secondary_rpc);
    info!("NEAR connections established");
    (primary_client, secondary_client)
}
pub async fn get_latest_block_height(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<u64, Box<dyn std::error::Error>> {
    let block_request = methods::block::RpcBlockRequest {
        block_reference: BlockReference::Finality(Finality::Final),
    };

    let block = query_rpc(primary_client, secondary_client, block_request, || {
        methods::block::RpcBlockRequest {
            block_reference: BlockReference::Finality(Finality::Final),
        }
    })
    .await?;

    Ok(block.header.height)
}
pub async fn query_rpc<M, F>(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    method: M,
    fallback: F,
) -> Result<M::Response, near_jsonrpc_client::errors::JsonRpcError<M::Error>>
where
    M: methods::RpcMethod,
    F: Fn() -> M,
    M::Error: std::fmt::Debug,
{
    info!("Querying RPC: {}", std::any::type_name::<M>());
    match primary_client.call(method).await {
        Ok(response) => {
            info!(
                "RPC query successful on primary: {}",
                std::any::type_name::<M>()
            );
            Ok(response)
        }
        Err(_) => {
            warn!("Primary RPC failed, trying secondary");
            match secondary_client.call(fallback()).await {
                Ok(response) => {
                    info!(
                        "RPC query successful on secondary: {}",
                        std::any::type_name::<M>()
                    );
                    Ok(response)
                }
                Err(e) => {
                    error!("Both RPCs failed: {:?}", e);
                    Err(e)
                }
            }
        }
    }
}

pub async fn get_accounts(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    validator_account_id: &str,
    block_height: u64,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let mut all_accounts = Vec::new();
    let mut from_index = 0;
    let limit = 1000;

    loop {
        info!(
            "Fetching accounts for block height {}, from_index: {}",
            block_height, from_index
        );
        let query_request = methods::query::RpcQueryRequest {
            block_reference: BlockReference::BlockId(near_primitives::types::BlockId::Height(
                block_height,
            )),
            request: near_primitives::views::QueryRequest::CallFunction {
                account_id: validator_account_id.parse()?,
                method_name: "get_accounts".to_string(),
                args: FunctionArgs::from(
                    serde_json::json!({ "from_index": from_index, "limit": limit })
                        .to_string()
                        .into_bytes(),
                ),
            },
        };

        let result = query_rpc(primary_client, secondary_client, query_request, || {
            methods::query::RpcQueryRequest {
                block_reference: BlockReference::BlockId(near_primitives::types::BlockId::Height(
                    block_height,
                )),
                request: near_primitives::views::QueryRequest::CallFunction {
                    account_id: validator_account_id.parse().unwrap(),
                    method_name: "get_accounts".to_string(),
                    args: FunctionArgs::from(
                        serde_json::json!({ "from_index": from_index, "limit": limit })
                            .to_string()
                            .into_bytes(),
                    ),
                },
            }
        })
        .await?;

        match result.kind {
            JsonRpcQueryResponseKind::CallResult(call_result) => {
                let accounts: Vec<serde_json::Value> = serde_json::from_slice(&call_result.result)?;
                all_accounts.extend(accounts.clone());

                if accounts.len() < limit as usize {
                    break;
                }

                from_index += limit;
            }
            _ => return Err("Unexpected query response kind".into()),
        }
    }

    Ok(all_accounts)
}

pub async fn get_block_info(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    height: u64,
) -> Result<(u64, BlockView), Box<dyn std::error::Error>> {
    let max_retries = 5;
    let mut retry_count = 0;
    let mut backoff_time = 1; // Start with 1 second
    let mut current_height = height;

    loop {
        info!(
            "Attempting to get block info for height: {}",
            current_height
        );
        let block_request = methods::block::RpcBlockRequest {
            block_reference: BlockReference::BlockId(near_primitives::types::BlockId::Height(
                current_height,
            )),
        };

        match query_rpc(primary_client, secondary_client, block_request, || {
            methods::block::RpcBlockRequest {
                block_reference: BlockReference::BlockId(near_primitives::types::BlockId::Height(
                    current_height,
                )),
            }
        })
        .await
        {
            Ok(block) => {
                info!(
                    "Successfully retrieved block info for height: {}",
                    current_height
                );
                return Ok((current_height, block));
            }
            Err(e) => {
                if e.to_string().contains("UNKNOWN_BLOCK") {
                    info!("Block {} not found, trying next block.", current_height);
                    current_height += 1;
                    retry_count = 0; // Reset retry count for new block
                    backoff_time = 1; // Reset backoff time for new block
                } else if e.to_string().contains("TooManyRequests") {
                    // Rate limit hit - back off and retry the same block
                    retry_count += 1;
                    if retry_count >= max_retries {
                        info!(
                            "Failed to get block {} after {} retries, moving to next block",
                            current_height, max_retries
                        );
                        current_height += 1;
                        retry_count = 0;
                        backoff_time = 1;
                        continue;
                    }

                    info!("Rate limit hit when getting block {}. Backing off for {} seconds (retry {}/{})", 
                          current_height, backoff_time, retry_count, max_retries);
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff_time)).await;
                    backoff_time *= 2; // Exponential backoff
                } else {
                    error!(
                        "Error getting block info for height {}: {:?}",
                        current_height, e
                    );
                    retry_count += 1;
                    if retry_count >= max_retries {
                        info!(
                            "Failed to get block {} after {} retries, moving to next block",
                            current_height, max_retries
                        );
                        current_height += 1;
                        retry_count = 0;
                        backoff_time = 1;
                        continue;
                    }

                    info!(
                        "Retrying after error ({}/{}). Waiting {} seconds",
                        retry_count, max_retries, backoff_time
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff_time)).await;
                    backoff_time *= 2; // Exponential backoff
                }
            }
        }
    }
}
pub async fn get_epoch_data(
    start_block_height: u64,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    batch_size: usize,
    epoch_blocks: u64,
) -> Result<Vec<EpochInfo>, Box<dyn std::error::Error>> {
    info!("===== EPOCH DATA GENERATION STARTED =====");
    info!(
        "Starting epoch data generation from block height: {}",
        start_block_height
    );

    // Get current block to know when to stop
    let current_block = get_latest_block_height(primary_client, secondary_client).await?;
    info!("Current block height: {}", current_block);

    // Get initial block and its epoch ID
    let (_, initial_block) =
        get_block_info(primary_client, secondary_client, start_block_height).await?;
    let initial_epoch_id = initial_block.header.epoch_id.to_string();
    info!(
        "Initial block {} has epoch ID: {}",
        start_block_height, initial_epoch_id
    );

    // Initialize data structures to track epochs
    let mut epochs = Vec::new();
    let mut current_height = start_block_height;
    let mut current_epoch_id = initial_epoch_id;
    let mut epoch_start_block = current_height;
    let mut epoch_timestamp = DateTime::<Utc>::from_utc(
        chrono::NaiveDateTime::from_timestamp_opt(
            (initial_block.header.timestamp / 1_000_000_000) as i64,
            0,
        )
        .unwrap(),
        Utc,
    );

    // Process epochs until we reach current block
    while current_height < current_block {
        // Estimate next epoch boundary using approximate epoch length
        let estimated_next_epoch_start = current_height + epoch_blocks;

        // If estimated boundary exceeds current block, we're done
        if estimated_next_epoch_start >= current_block {
            break;
        }

        let boundary = find_epoch_boundary(
            current_height,
            estimated_next_epoch_start + epoch_blocks / 2, // Add some buffer
            &current_epoch_id,
            primary_client,
            secondary_client,
        )
        .await?;

        info!(
            "Found epoch boundary: Current epoch {} ends at block {}",
            current_epoch_id,
            boundary - 1
        );

        // Get the new epoch ID from the boundary block
        let (_, boundary_block) =
            get_block_info(primary_client, secondary_client, boundary).await?;
        let next_epoch_id = boundary_block.header.epoch_id.to_string();

        // Record the current epoch
        epochs.push(EpochInfo {
            start_block: epoch_start_block,
            end_block: Some(boundary - 1),
            epoch_id: current_epoch_id,
            timestamp: epoch_timestamp,
        });

        // Update tracking variables for next epoch
        current_height = boundary;
        current_epoch_id = next_epoch_id;
        epoch_start_block = boundary;
        epoch_timestamp = DateTime::<Utc>::from_utc(
            chrono::NaiveDateTime::from_timestamp_opt(
                (boundary_block.header.timestamp / 1_000_000_000) as i64,
                0,
            )
            .unwrap(),
            Utc,
        );

        info!(
            "New epoch {} starts at block {}",
            current_epoch_id, epoch_start_block
        );

        // Add a small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // Add the final epoch (partial) that reaches to the current block
    if epoch_start_block < current_block {
        info!(
            "Adding final partial epoch {} from {} to {}",
            current_epoch_id, epoch_start_block, current_block
        );

        epochs.push(EpochInfo {
            start_block: epoch_start_block,
            end_block: Some(current_block),
            epoch_id: current_epoch_id,
            timestamp: epoch_timestamp,
        });
    }

    info!("===== EPOCH DATA GENERATION COMPLETE =====");
    info!("Generated {} epoch boundaries", epochs.len());

    for (i, epoch) in epochs.iter().enumerate() {
        info!(
            "Epoch {} (ID: {}) - Start: {}, End: {}",
            i + 1,
            epoch.epoch_id,
            epoch.start_block,
            epoch.end_block.unwrap_or(0)
        );
    }

    Ok(epochs)
}

async fn find_epoch_boundary(
    start_block: u64,
    end_block: u64,
    current_epoch_id: &str,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<u64, Box<dyn std::error::Error>> {
    info!(
        "Binary searching for epoch boundary between blocks {} and {}",
        start_block, end_block
    );

    let mut low = start_block;
    let mut high = end_block;

    while low <= high {
        if high - low <= 5 {
            // When range is small, do linear search for more accuracy
            return find_boundary_linear(
                low,
                high,
                current_epoch_id,
                primary_client,
                secondary_client,
            )
            .await;
        }

        let mid = low + (high - low) / 2;
        info!("Checking block {}", mid);

        // Get epoch ID for the middle block
        match get_block_info(primary_client, secondary_client, mid).await {
            Ok((actual_height, block)) => {
                let mid_epoch_id = block.header.epoch_id.to_string();

                if mid_epoch_id == current_epoch_id {
                    // Still in the same epoch, boundary is higher
                    low = actual_height + 1;
                } else {
                    // We've crossed into a new epoch, boundary is lower
                    high = actual_height - 1;
                }
            }
            Err(_) => {
                // If block retrieval fails, try the next block
                info!("Failed to get block {}, trying next block", mid);
                low = mid + 1;
            }
        }

        // Add a small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // If we get here, low is the boundary
    Ok(low)
}
async fn find_boundary_linear(
    start_block: u64,
    end_block: u64,
    current_epoch_id: &str,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
) -> Result<u64, Box<dyn std::error::Error>> {
    info!(
        "Linear searching for exact boundary between blocks {} and {}",
        start_block, end_block
    );

    let mut current = start_block;

    while current <= end_block {
        match get_block_info(primary_client, secondary_client, current).await {
            Ok((_, block)) => {
                let block_epoch_id = block.header.epoch_id.to_string();

                if block_epoch_id != current_epoch_id {
                    // Found the boundary
                    return Ok(current);
                }
            }
            Err(_) => {
                // If block retrieval fails, try the next block
                info!("Failed to get block {}, trying next block", current);
            }
        }

        current += 1;

        // Add a small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // If we didn't find a boundary, return the block after the end
    Ok(end_block + 1)
}

async fn batch_query_blocks(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    heights: Vec<u64>,
) -> Vec<(u64, BlockView)> {
    let mut results = Vec::new();
    for height in heights {
        if let Ok(result) = get_block_info(primary_client, secondary_client, height).await {
            results.push(result);
        }
    }
    results
}

// async fn find_epoch_start_blocks(
//     primary_client: &JsonRpcClient,
//     secondary_client: &JsonRpcClient,
//     start_block_height: u64,
//     batch_size: usize,
//     epoch_blocks: u64,
// ) -> Result<Vec<EpochInfo>, Box<dyn std::error::Error>> {
//     let block_request = methods::block::RpcBlockRequest {
//         block_reference: BlockReference::Finality(Finality::Final),
//     };
//     let current_block = query_rpc(primary_client, secondary_client, block_request, || {
//         methods::block::RpcBlockRequest {
//             block_reference: BlockReference::Finality(Finality::Final),
//         }
//     })
//     .await?;
//     let current_block_height = current_block.header.height;
//     info!("Current block height: {}", current_block_height);
//     info!("Starting search from block height: {}", start_block_height);

//     let mut epoch_start_blocks = Vec::new();
//     let mut current_height = start_block_height;
//     let mut epoch_counter = 0;

//     while current_height <= current_block_height {
//         let mut batch_heights = Vec::new();
//         for i in 0..batch_size {
//             let height = current_height + i as u64 * epoch_blocks;
//             if height <= current_block_height {
//                 batch_heights.push(height);
//             } else {
//                 break;
//             }
//         }

//         let batch_results =
//             batch_query_blocks(primary_client, secondary_client, batch_heights).await;

//         for (i, (height, block)) in batch_results.iter().enumerate() {
//             if i == 0 || block.header.epoch_id != batch_results[i - 1].1.header.epoch_id {
//                 epoch_counter += 1;
//                 info!("Epoch {} starts at block: {}", epoch_counter, height);

//                 epoch_start_blocks.push(EpochInfo {
//                     start_block: *height,
//                     epoch_id: block.header.epoch_id.to_string(),
//                     timestamp: DateTime::<Utc>::from_utc(
//                         chrono::NaiveDateTime::from_timestamp_opt(
//                             (block.header.timestamp / 1_000_000_000) as i64,
//                             0,
//                         )
//                         .unwrap(),
//                         Utc,
//                     ),
//                     end_block: None,
//                 });
//             }
//         }

//         current_height = batch_results
//             .last()
//             .map(|(height, _)| height + epoch_blocks)
//             .unwrap_or(current_height);
//         info!("Progressed to block: {}", current_height);
//     }

//     // Set end_block for each epoch
//     for i in 0..epoch_start_blocks.len() - 1 {
//         epoch_start_blocks[i].end_block = Some(epoch_start_blocks[i + 1].start_block - 1);
//     }

//     Ok(epoch_start_blocks)
// }
