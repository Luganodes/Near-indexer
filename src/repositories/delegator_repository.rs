use crate::models::DelegatorData;
use mongodb::bson::{doc, to_bson, Bson};
use mongodb::options::UpdateOptions;
use mongodb::{Collection, Database};

pub async fn save_delegator_data(
    db: &Database,
    delegator_data: &[DelegatorData],
    batch_size: usize,
) -> Result<(), mongodb::error::Error> {
    let collection: Collection<DelegatorData> = db.collection("delegators");

    for chunk in delegator_data.chunks(batch_size) {
        for data in chunk {
            let filter = doc! {
                "delegatorId": &data.delegator_id,
                "validatorAccountId": &data.validator_account_id,
                "epoch": Bson::Int64(data.epoch as i64),
            };
            let update = doc! {
                "$set": to_bson(data)?
            };
            let options = UpdateOptions::builder().upsert(Some(true)).build();
            collection
                .update_one(filter, update)
                .upsert(options.upsert.unwrap_or(false))
                .await?;
        }
    }

    Ok(())
}
