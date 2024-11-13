use dotenv::dotenv;
use futures::stream::{self, StreamExt};
use log::{error, info};
use mongodb::Database;
use near_jsonrpc_client::JsonRpcClient;
use std::sync::Arc;

mod config;
mod models;
mod repositories;
mod services;
mod transaction_fetcher;
mod utils;

use crate::config::Config;
use crate::models::{EpochInfo, Transaction};
use crate::repositories::epoch_sync_repository;
use crate::services::{database, epoch_processor, near_rpc};
use crate::transaction_fetcher::fetch_and_process_transactions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    env_logger::init();

    info!("Starting NEAR indexer script");
    let config = Arc::new(Config::from_env());

    info!("Connecting to NEAR network...");
    let (primary_client, secondary_client) =
        near_rpc::create_near_connections(&config.primary_rpc, &config.secondary_rpc).await;
    let clients = Arc::new((primary_client, secondary_client));
    info!("Connected to NEAR network");

    let db = database::connect_to_database().await?;

    info!("Fetching and processing transactions...");
    let new_transactions =
        fetch_and_process_transactions(&config, &db, &clients.0, &clients.1).await?;

    let start_block_height = new_transactions
        .iter()
        .map(|tx| tx.block_height)
        .min()
        .unwrap_or_else(|| panic!("No transactions found"));

    info!("Starting from block height: {}", start_block_height);

    let transactions: Arc<Vec<Transaction>> = Arc::new(new_transactions);

    info!("Getting epoch data...");
    let epoch_data = Arc::new(
        get_or_sync_epoch_data(
            &db,
            start_block_height,
            &clients.0,
            &clients.1,
            config.batch_size,
            config.epoch_blocks,
        )
        .await?,
    );

    let validator_account_id = config.validator_account_id.clone();
    let epoch_data_clone = Arc::clone(&epoch_data);
    let config_clone = Arc::clone(&config);
    let process_epoch_tasks = stream::iter(epoch_data_clone.iter().enumerate())
        .map(move |(index, epoch)| {
            let clients = Arc::clone(&clients);
            let transactions = Arc::clone(&transactions);
            let epoch_data = Arc::clone(&epoch_data);
            let db = db.clone();
            let validator_account_id = validator_account_id.clone();
            let config = Arc::clone(&config_clone);
            async move {
                info!("Processing epoch {}: {:?}", index + 1, epoch);
                let next_epoch = epoch_data.get(index + 1);
                let end_block = next_epoch.map(|e| e.start_block - 1).unwrap_or(u64::MAX);

                epoch_processor::process_delegator_data(
                    &clients.0,
                    &clients.1,
                    &validator_account_id,
                    epoch.start_block,
                    end_block,
                    &transactions,
                    index as u64 + 1,
                    &epoch.epoch_id,
                    epoch.timestamp.timestamp_millis() as u64,
                    &db,
                    &config,
                )
                .await
            }
        })
        .buffer_unordered(config.parallel_limit)
        .collect::<Vec<_>>()
        .await;

    for result in process_epoch_tasks {
        if let Err(e) = result {
            error!("Error processing epoch: {:?}", e);
        }
    }

    info!("Processing complete. Data has been saved to MongoDB.");
    Ok(())
}

async fn get_or_sync_epoch_data(
    db: &Database,
    start_block_height: u64,
    primary_client: &JsonRpcClient,
    secondary_client: &JsonRpcClient,
    batch_size: usize,
    epoch_blocks: u64,
) -> Result<Vec<EpochInfo>, Box<dyn std::error::Error>> {
    let latest_epoch_sync = epoch_sync_repository::get_latest_epoch_sync(db).await?;
    let epoch_sync_count = epoch_sync_repository::get_epoch_sync_count(db).await?;

    if let Some(latest) = latest_epoch_sync {
        let current_block =
            near_rpc::get_latest_block_height(primary_client, secondary_client).await?;
        if current_block - latest.start_block > epoch_blocks {
            // More than one epoch has passed, sync from the last known epoch
            let new_epochs = near_rpc::get_epoch_data(
                latest.start_block,
                primary_client,
                secondary_client,
                batch_size,
                epoch_blocks,
            )
            .await?;

            for epoch in &new_epochs {
                epoch_sync_repository::save_epoch_sync(db, epoch).await?;
            }

            let mut all_epochs = Vec::with_capacity(epoch_sync_count as usize + new_epochs.len());
            for i in 0..epoch_sync_count {
                if let Some(epoch) = epoch_sync_repository::get_epoch_sync_by_index(db, i).await? {
                    all_epochs.push(epoch);
                }
            }
            all_epochs.extend(new_epochs);
            Ok(all_epochs)
        } else {
            // Less than one epoch has passed, use existing data
            let mut all_epochs = Vec::with_capacity(epoch_sync_count as usize);
            for i in 0..epoch_sync_count {
                if let Some(epoch) = epoch_sync_repository::get_epoch_sync_by_index(db, i).await? {
                    all_epochs.push(epoch);
                }
            }
            Ok(all_epochs)
        }
    } else {
        // No existing data, sync from the start
        let epochs = near_rpc::get_epoch_data(
            start_block_height,
            primary_client,
            secondary_client,
            batch_size,
            epoch_blocks,
        )
        .await?;

        for epoch in &epochs {
            epoch_sync_repository::save_epoch_sync(db, epoch).await?;
        }

        Ok(epochs)
    }
}
