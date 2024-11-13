use crate::models::Transaction;
use futures::StreamExt;
use mongodb::options::FindOptions;
use mongodb::{bson::doc, Collection, Database};

pub async fn save_transactions(
    db: &Database,
    transactions: &[Transaction],
) -> Result<(), mongodb::error::Error> {
    let collection: Collection<Transaction> = db.collection("transactions");
    collection.insert_many(transactions).await?;
    Ok(())
}

pub async fn get_latest_transaction(
    db: &Database,
) -> Result<Option<Transaction>, mongodb::error::Error> {
    let collection: Collection<Transaction> = db.collection("transactions");
    let options = FindOptions::builder()
        .sort(doc! { "block_height": -1 })
        .limit(1)
        .build();
    let mut cursor = collection
        .find(doc! {})
        .sort(options.sort.unwrap_or_default())
        .limit(options.limit.unwrap_or(1))
        .await?;
    cursor.next().await.transpose()
}
