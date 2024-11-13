use crate::models::{DelegatorData, Transaction};
use mongodb::options::UpdateOptions;

use mongodb::{
    bson::{doc, to_bson},
    Collection, Database,
};
use std::collections::HashMap;

pub async fn save_epoch_data(
    db: &Database,
    epoch: u64,
    epoch_id: &str,
    delegator_data: &HashMap<String, DelegatorData>,
    validator_account_id: &str,
    start_block_height: u64,
    end_block_height: u64,
    epoch_transactions: &[&Transaction],
    epoch_timestamp: u64,
) -> Result<(), mongodb::error::Error> {
    let collection: Collection<mongodb::bson::Document> = db.collection("epoch_data");
    let epoch_data = doc! {
        "epoch": epoch as i64,
        "epochId": epoch_id,
        "validatorAccountId": validator_account_id,
        "startBlockHeight": start_block_height as i64,
        "endBlockHeight": end_block_height as i64,
        "timestamp": mongodb::bson::DateTime::from_millis(epoch_timestamp as i64),
        "delegators": to_bson(delegator_data)?,
        "transactions": to_bson(epoch_transactions)?,
    };

    let options = UpdateOptions::builder().upsert(Some(true)).build();
    collection.update_one(
        doc! { "epoch": epoch as i64, "epochId": epoch_id, "validatorAccountId": validator_account_id },
        doc! { "$set": epoch_data },
    ).upsert(options.upsert.unwrap_or(false)).await?;
    Ok(())
}
