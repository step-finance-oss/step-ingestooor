use rust_decimal::{prelude::FromPrimitive, Decimal};
use serde::{
    de::Error as DeError, ser::Error as SerError, Deserialize, Deserializer, Serialize, Serializer,
};

pub fn serialize_ch_decimal_u64<S: Serializer>(
    value: &Decimal,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let val_u64: u64 = (*value).try_into().map_err(|_| {
        S::Error::custom(format!("Failed to convert Decimal to u64, got {}", value))
    })?;
    val_u64.serialize(serializer)
}

pub fn deserialize_ch_decimal_u64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Decimal, D::Error> {
    let val_u64: u64 = u64::deserialize(deserializer)?;
    Ok(Decimal::from(val_u64))
}

pub fn serialize_ch_decimal_u64_opt<S: Serializer>(
    value: &Option<Decimal>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let val_opt_u64: Option<u64> = value
        .map(|v| {
            v.try_into().map_err(|_| {
                S::Error::custom(format!("Failed to convert opt Decimal to u64, got {}", v))
            })
        })
        .transpose()?;

    val_opt_u64.serialize(serializer)
}

pub fn deserialize_ch_decimal_u64_opt<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Decimal>, D::Error> {
    let val_opt_u64: Option<u64> = Option::deserialize(deserializer)?;
    Ok(val_opt_u64.map(Decimal::from))
}

pub fn serialize_ch_decimal_vec_u64<S: Serializer>(
    value: &[Decimal],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let vec_u64: Vec<u64> = value
        .iter()
        .map(|v| {
            (*v).try_into().map_err(|_| {
                S::Error::custom(format!("Failed to convert vec Decimal to u64, got {}", v))
            })
        })
        .collect::<Result<Vec<u64>, S::Error>>()?;
    vec_u64.serialize(serializer)
}

pub fn deserialize_ch_decimal_vec_u64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Decimal>, D::Error> {
    let vec_u64: Vec<u64> = Vec::deserialize(deserializer)?;
    Ok(vec_u64.iter().map(|v| Decimal::from(*v)).collect())
}

pub fn serialize_ch_decimal_f64<S: Serializer>(
    value: &Decimal,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let val_f64: f64 = (*value).try_into().map_err(|_| {
        S::Error::custom(format!("Failed to convert Decimal to f64, got {}", value))
    })?;
    val_f64.serialize(serializer)
}

pub fn deserialize_ch_decimal_f64<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Decimal, D::Error> {
    let val_f64: f64 = f64::deserialize(deserializer)?;
    Decimal::from_f64(val_f64).ok_or_else(|| {
        D::Error::custom(format!(
            "Bad f64 passed to deserialize Decimal, got {}",
            val_f64
        ))
    })
}
