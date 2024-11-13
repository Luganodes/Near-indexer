use log::info;
use mongodb::{Client, Database};
use std::env;

pub async fn connect_to_database() -> mongodb::error::Result<Database> {
    let mongo_uri = env::var("MONGO_URI").expect("MONGO_URI must be set");
    let db_name = env::var("DB_NAME").expect("DB_NAME must be set");
    let client = Client::with_uri_str(&mongo_uri).await?;
    info!("Connected to MongoDB");
    info!("a {}", db_name);
    Ok(client.database(&db_name))
}
