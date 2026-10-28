use chrono::NaiveDateTime;
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize_ch_naive_date_time<S: Serializer>(
    value: &NaiveDateTime,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let ts = value.and_utc().timestamp_micros();
    ts.serialize(serializer)
}

#[allow(deprecated)]
pub fn deserialize_ch_naive_date_time<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<NaiveDateTime, D::Error> {
    let ts = i64::deserialize(deserializer)?;
    let dt = NaiveDateTime::from_timestamp_micros(ts).ok_or(D::Error::custom(
        "Invalid timestamp (ensure that UTC and microseconds are used)",
    ))?;

    Ok(dt)
}

pub fn serialize_ch_naive_date_time_opt<S: Serializer>(
    value: &Option<NaiveDateTime>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let ts = value.map(|v| v.and_utc().timestamp_micros());
    ts.serialize(serializer)
}

#[allow(deprecated)]
pub fn deserialize_ch_naive_date_time_opt<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NaiveDateTime>, D::Error> {
    let ts = Option::<i64>::deserialize(deserializer)?;

    let result = ts
        .map(|ts| {
            NaiveDateTime::from_timestamp_micros(ts).ok_or(D::Error::custom(
                "Invalid timestamp (ensure that UTC and microseconds are used)",
            ))
        })
        .transpose()?;

    Ok(result)
}
