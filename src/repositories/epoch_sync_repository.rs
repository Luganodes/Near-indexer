use crate::models::EpochInfo;
use futures::StreamExt;
use mongodb::bson::{doc, to_document};
use mongodb::options::{FindOptions, UpdateOptions};
use mongodb::{Collection, Database};

pub async fn save_epoch_sync(
    db: &Database,
    epoch_info: &EpochInfo,
) -> Result<(), mongodb::error::Error> {
    let collection: Collection<EpochInfo> = db.collection("epoch_sync");
    let filter = doc! { "epoch_id": &epoch_info.epoch_id };
    let update = doc! { "$set": to_document(epoch_info)? };
    let options = UpdateOptions::builder().upsert(true).build();
    collection
        .update_one(filter, update)
        .upsert(options.upsert.unwrap_or(false))
        .await?;
    Ok(())
}

pub async fn get_latest_epoch_sync(
    db: &Database,
) -> Result<Option<EpochInfo>, mongodb::error::Error> {
    let collection: Collection<EpochInfo> = db.collection("epoch_sync");
    let options = FindOptions::builder()
        .sort(doc! { "start_block": -1 })
        .limit(1)
        .build();
    let mut cursor = collection
        .find(doc! {})
        .sort(options.sort.unwrap_or_default())
        .limit(options.limit.unwrap_or(1))
        .await?;
    cursor.next().await.transpose()
}

pub async fn get_epoch_sync_count(db: &Database) -> Result<u64, mongodb::error::Error> {
    let collection: Collection<EpochInfo> = db.collection("epoch_sync");
    collection.count_documents(doc! {}).await
}

pub async fn get_epoch_sync_by_index(
    db: &Database,
    index: u64,
) -> Result<Option<EpochInfo>, mongodb::error::Error> {
    let collection: Collection<EpochInfo> = db.collection("epoch_sync");
    let options = FindOptions::builder()
        .sort(doc! { "start_block": 1 })
        .skip(Some(index))
        .limit(1)
        .build();
    let mut cursor = collection
        .find(doc! {})
        .sort(options.sort.unwrap_or_default())
        .skip(options.skip.unwrap_or(0))
        .limit(options.limit.unwrap_or(1))
        .await?;
    cursor.next().await.transpose()
}
