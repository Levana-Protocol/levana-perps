use super::{NonZero, Signed, UnsignedDecimal};
use schemars::schema::{InstanceType, SchemaObject};
use schemars::JsonSchema;
use serde::{de, ser, Deserialize, Deserializer, Serialize};
use std::fmt;
use std::marker::PhantomData;

/// Serializes as a string for serde
impl<T: UnsignedDecimal> Serialize for Signed<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Deserializes as string for serde
impl<'de, T: UnsignedDecimal> Deserialize<'de> for Signed<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(SignedVisitor(PhantomData))
    }
}

struct SignedVisitor<T>(PhantomData<T>);

impl<'de, T: UnsignedDecimal> de::Visitor<'de> for SignedVisitor<T> {
    type Value = Signed<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string-encoded signed decimal")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v.parse() {
            Ok(d) => Ok(d),
            Err(e) => Err(E::custom(format!(
                "Error parsing signed decimal '{}': {}",
                v, e
            ))),
        }
    }
}

impl<T: UnsignedDecimal> Serialize for NonZero<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de, T: UnsignedDecimal> Deserialize<'de> for NonZero<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(NonZeroVisitor(PhantomData))
    }
}

struct NonZeroVisitor<T>(PhantomData<T>);

impl<'de, T: UnsignedDecimal> de::Visitor<'de> for NonZeroVisitor<T> {
    type Value = NonZero<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("string-encoded decimal greater than zero")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v.parse() {
            Ok(d) => Ok(d),
            Err(e) => Err(E::custom(format!(
                "Error parsing non-zero decimal '{}': {}",
                v, e
            ))),
        }
    }
}

impl<T: UnsignedDecimal> JsonSchema for NonZero<T> {
    fn schema_name() -> String {
        "NonZero decimal".to_owned()
    }

    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            format: Some("non-zero".to_owned()),
            ..Default::default()
        }
        .into()
    }
}
