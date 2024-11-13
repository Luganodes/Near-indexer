use crate::models::Transaction;
use std::fs;

pub fn load_transactions(file_path: &str) -> Result<Vec<Transaction>, Box<dyn std::error::Error>> {
    let raw_data = fs::read_to_string(file_path)?;
    let transactions: Vec<Transaction> = serde_json::from_str(&raw_data)?;
    Ok(transactions)
}
