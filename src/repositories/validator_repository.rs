use crate::models::DelegatorData;
use mongodb::options::UpdateOptions;
use mongodb::{bson::doc, Collection, Database};
use num_bigint::BigInt;
use num_traits::Zero;
use std::collections::HashMap;
use std::str::FromStr; // Add this import

pub async fn save_validator_metrics(
    db: &Database,
    validator_account_id: &str,
    epoch: u64,
    epoch_id: &str,
    delegator_data: &HashMap<String, DelegatorData>,
    epoch_timestamp: u64,
    apy: f64, // Added APY parameter
) -> Result<(), mongodb::error::Error> {
    let collection: Collection<mongodb::bson::Document> = db.collection("validator_metrics");

    let mut total_staked = BigInt::from(0);
    let total_delegators = delegator_data.len() as i64;

    for data in delegator_data.values() {
        total_staked +=
            BigInt::from_str(&data.auto_compounded_stake).unwrap_or_else(|_| BigInt::zero());
    }

    let metrics = doc! {
        "validatorAccountId": validator_account_id,
        "epoch": epoch as i64,
        "epochId": epoch_id,
        "totalStaked": total_staked.to_string(),
        "totalDelegators": total_delegators,
        "timestamp": mongodb::bson::DateTime::from_millis(epoch_timestamp as i64),
        "apy": apy,  // Added APY to metrics
    };

    let options = UpdateOptions::builder().upsert(true).build();
    collection
        .update_one(
            doc! {
                "validatorAccountId": validator_account_id,
                "epoch": epoch as i64,
                "epochId": epoch_id
            },
            doc! {
                "$set": metrics.clone(),
                "$push": {
                    "history": {
                        "$each": [metrics],
                        "$slice": -100,
                    }
                }
            },
        )
        .upsert(options.upsert.unwrap_or(false))
        .await?;

    Ok(())
}
