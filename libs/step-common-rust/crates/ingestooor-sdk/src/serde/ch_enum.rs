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
    CurveType::deserialize(deserializer)
}

#[cfg(test)]
mod tests {
    use serde::de::value::{Error, U16Deserializer};

    use super::*;

    #[test]
    fn curve_codes_keep_the_existing_wire_mapping() {
        let curve = |code| deserialize_curve_type(U16Deserializer::<Error>::new(code));
        assert_eq!(curve(0).ok(), Some(CurveType::ConstantProduct));
        assert_eq!(curve(4).ok(), Some(CurveType::ConcentratedLiquidity));
        assert_eq!(curve(6).ok(), Some(CurveType::Unknown));
        assert_eq!(curve(7).ok(), Some(CurveType::DynamicLiquidity));
        assert!(curve(8).is_err());
    }
}
