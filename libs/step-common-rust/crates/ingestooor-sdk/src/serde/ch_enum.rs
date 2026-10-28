use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::dooot::CurveType;

pub fn serialize_curve_type<S: Serializer>(
    value: &CurveType,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    (value.clone() as u16).serialize(serializer)
}

pub fn deserialize_curve_type<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<CurveType, D::Error> {
    let value = u16::deserialize(deserializer)?;
    let curve_type = unsafe { std::mem::transmute::<u16, CurveType>(value) };

    Ok(curve_type)
}
