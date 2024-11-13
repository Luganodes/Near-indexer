use crate::models::EpochInfo;
use chrono::{DateTime, Utc};
use log::{error, info, warn};
use near_jsonrpc_client::{methods, JsonRpcClient};
use near_jsonrpc_primitives::types::query::QueryResponseKind as JsonRpcQueryResponseKind;
use near_primitives::types::{BlockReference, Finality, FunctionArgs};
use near_primitives::views::BlockView;

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
    mut height: u64,
) -> Result<(u64, BlockView), Box<dyn std::error::Error>> {
    loop {
        info!("Attempting to get block info for height: {}", height);
        let block_request = methods::block::RpcBlockRequest {
            block_reference: BlockReference::BlockId(near_primitives::types::BlockId::Height(
                height,
            )),
        };

        match query_rpc(primary_client, secondary_client, block_request, || {
            methods::block::RpcBlockRequest {
                block_reference: BlockReference::BlockId(near_primitives::types::BlockId::Height(
                    height,
                )),
            }
        })
        .await
        {
            Ok(block) => {
                info!("Successfully retrieved block info for height: {}", height);
                return Ok((height, block));
            }
            Err(e) => {
                if e.to_string().contains("UNKNOWN_BLOCK") {
                    info!("Block {} not found, trying next block.", height);
                    height += 1;
                } else {
                    error!("Error getting block info for height {}: {:?}", height, e);
                    info!("Moving to next block.");
                    height += 1;
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
    info!("Generating epoch data...");
    let epoch_start_blocks = find_epoch_start_blocks(
        primary_client,
        secondary_client,
        start_block_height,
        batch_size,
        epoch_blocks,
    )
    .await?;
    info!(
        "Epoch start blocks: {:?}",
        &epoch_start_blocks[..5.min(epoch_start_blocks.len())]
    );
    Ok(epoch_start_blocks)
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

async fn find_epoch_start_blocks(
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    start_block_height: u64,
    batch_size: usize,
    epoch_blocks: u64,
) -> Result<Vec<EpochInfo>, Box<dyn std::error::Error>> {
    let block_request = methods::block::RpcBlockRequest {
        block_reference: BlockReference::Finality(Finality::Final),
    };
    let current_block = query_rpc(primary_client, secondary_client, block_request, || {
        methods::block::RpcBlockRequest {
            block_reference: BlockReference::Finality(Finality::Final),
        }
    })
    .await?;
    let current_block_height = current_block.header.height;
    info!("Current block height: {}", current_block_height);
    info!("Starting search from block height: {}", start_block_height);

    let mut epoch_start_blocks = Vec::new();
    let mut current_height = start_block_height;
    let mut epoch_counter = 0;

    while current_height <= current_block_height {
        let mut batch_heights = Vec::new();
        for i in 0..batch_size {
            let height = current_height + i as u64 * epoch_blocks;
            if height <= current_block_height {
                batch_heights.push(height);
            } else {
                break;
            }
        }

        let batch_results =
            batch_query_blocks(primary_client, secondary_client, batch_heights).await;

        for (i, (height, block)) in batch_results.iter().enumerate() {
            if i == 0 || block.header.epoch_id != batch_results[i - 1].1.header.epoch_id {
                epoch_counter += 1;
                info!("Epoch {} starts at block: {}", epoch_counter, height);

                epoch_start_blocks.push(EpochInfo {
                    start_block: *height,
                    epoch_id: block.header.epoch_id.to_string(),
                    timestamp: DateTime::<Utc>::from_utc(
                        chrono::NaiveDateTime::from_timestamp_opt(
                            (block.header.timestamp / 1_000_000_000) as i64,
                            0,
                        )
                        .unwrap(),
                        Utc,
                    ),
                    end_block: None,
                });
            }
        }

        current_height = batch_results
            .last()
            .map(|(height, _)| height + epoch_blocks)
            .unwrap_or(current_height);
        info!("Progressed to block: {}", current_height);
    }

    // Set end_block for each epoch
    for i in 0..epoch_start_blocks.len() - 1 {
        epoch_start_blocks[i].end_block = Some(epoch_start_blocks[i + 1].start_block - 1);
    }

    Ok(epoch_start_blocks)
}
