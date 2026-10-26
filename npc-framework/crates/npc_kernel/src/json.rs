//! [`JsonObject`]: a JSON object that keeps its field order.
//!
//! `serde_json::Map` sorts its keys (the workspace does not enable
//! `preserve_order`), so a content command or effect stored as a `Value`
//! would serialize `{"type":"SetFood","food":0,"stockpile":3}` instead of the
//! typed struct's `{"type":"SetFood","stockpile":3,"food":0}`. Journals and
//! replays must stay byte-identical whether a value went through the generic
//! layer or not, so payloads keep the field order of the typed struct that
//! produced them.

use std::fmt;

use serde::de::{DeserializeOwned, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// An ordered JSON object (top-level field order preserved; nested objects
/// are plain `Value`s).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct JsonObject(pub Vec<(String, Value)>);

impl JsonObject {
    /// The object a typed value serializes to, in its field order.
    ///
    /// # Errors
    /// When `value` does not serialize to a JSON object.
    pub fn from_typed<T: Serialize>(value: &T) -> Result<Self, String> {
        // Through text, so the struct's field order survives.
        let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| format!("not a JSON object: {e}"))
    }

    /// Deserializes the object into `T`.
    ///
    /// # Errors
    /// The serde error, as text.
    pub fn to_typed<T: DeserializeOwned>(&self) -> Result<T, String> {
        let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }

    /// The field `key`, if present.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Removes and returns the field `key`.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let i = self.0.iter().position(|(k, _)| k == key)?;
        Some(self.0.remove(i).1)
    }

    /// As a (key-sorted) `serde_json::Value`.
    pub fn to_value(&self) -> Value {
        Value::Object(self.0.iter().cloned().collect())
    }
}

impl Serialize for JsonObject {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for JsonObject {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = JsonObject;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<JsonObject, A::Error> {
                let mut out = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some((k, v)) = map.next_entry::<String, Value>()? {
                    out.push((k, v));
                }
                Ok(JsonObject(out))
            }
        }
        d.deserialize_map(V)
    }
}
