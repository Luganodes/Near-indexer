use chrono::{DateTime, TimeZone, Utc};
use mongodb::bson::{self, Bson};
use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize_datetime<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let timestamp_ms = dt.timestamp_millis();
    let bson_dt = Bson::DateTime(bson::DateTime::from_millis(timestamp_ms));
    bson_dt.serialize(serializer)
}

pub fn deserialize_datetime<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let bson = Bson::deserialize(deserializer)?;
    match bson {
        Bson::DateTime(dt) => Ok(Utc.timestamp_millis_opt(dt.timestamp_millis()).unwrap()),
        _ => Err(serde::de::Error::custom("expecting DateTime")),
    }
}
