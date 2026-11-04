//! Complete enum-keyed maps. Saves store them as JSON objects keyed by the
//! stable serde variant name, so reordering or appending enum variants never
//! reinterprets a saved counter the way a positional array would.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::ops::{Index, IndexMut};

/// A closed set of stable kinds; `ALL` lists every key a [`KindMap`] holds.
pub trait StableKind: Copy + Ord + 'static {
    const ALL: &'static [Self];
}

/// A `BTreeMap` that always holds every kind. Missing keys in saved data are
/// filled with `V::default()`; iteration follows the enum's declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindMap<K: StableKind, V>(BTreeMap<K, V>);

impl<K: StableKind, V> KindMap<K, V> {
    pub fn from_fn(mut value: impl FnMut(K) -> V) -> Self {
        Self(K::ALL.iter().map(|&kind| (kind, value(kind))).collect())
    }
    pub fn get(&self, kind: K) -> &V {
        &self[kind]
    }
    /// Every `(kind, value)` pair in kind order.
    pub fn entries(&self) -> impl Iterator<Item = (K, &V)> {
        self.0.iter().map(|(kind, value)| (*kind, value))
    }
    /// Values in kind order, like iterating the array this map replaced.
    pub fn iter(&self) -> impl Iterator<Item = &V> {
        self.0.values()
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.0.values()
    }
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.0.values_mut()
    }
}

impl<K: StableKind, V: Clone> KindMap<K, V> {
    pub fn filled(value: V) -> Self {
        Self::from_fn(|_| value.clone())
    }
}

impl<K: StableKind, V: Default> Default for KindMap<K, V> {
    fn default() -> Self {
        Self::from_fn(|_| V::default())
    }
}

impl<K: StableKind, V> Index<K> for KindMap<K, V> {
    type Output = V;
    fn index(&self, kind: K) -> &V {
        self.0.get(&kind).expect("KindMap holds every kind")
    }
}

impl<K: StableKind, V> IndexMut<K> for KindMap<K, V> {
    fn index_mut(&mut self, kind: K) -> &mut V {
        self.0.get_mut(&kind).expect("KindMap holds every kind")
    }
}

impl<K: StableKind + Serialize, V: Serialize> Serialize for KindMap<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, K, V> Deserialize<'de> for KindMap<K, V>
where
    K: StableKind + Deserialize<'de>,
    V: Default + Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut saved = BTreeMap::<K, V>::deserialize(deserializer)?;
        Ok(Self::from_fn(|kind| {
            saved.remove(&kind).unwrap_or_default()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::StructureKind;

    #[test]
    fn serializes_as_a_name_keyed_object_and_fills_missing_kinds() {
        let mut map = KindMap::<StructureKind, u32>::default();
        map[StructureKind::Wall] = 2;
        assert_eq!(
            serde_json::to_string(&map).unwrap(),
            r#"{"Trap":0,"Wall":2,"Turret":0}"#
        );
        let sparse: KindMap<StructureKind, u32> = serde_json::from_str(r#"{"Turret":3}"#).unwrap();
        assert_eq!(
            sparse.entries().collect::<Vec<_>>(),
            [
                (StructureKind::Trap, &0),
                (StructureKind::Wall, &0),
                (StructureKind::Turret, &3)
            ]
        );
        assert!(serde_json::from_str::<KindMap<StructureKind, u32>>(r#"{"Moat":1}"#).is_err());
    }
}
