//! [`StableRegistry`]: names → dense ids that every build with the same features agrees on
//! (ADR-0004).
//!
//! Features register namespaced `&'static str` names (`"core.move"`). [`freeze`] sorts them, so
//! a name's dense id is its position in name order, and precomputes each name's 64-bit key
//! ([`fl_math::stream_key`]). Anything hashed uses the key, never the dense id, so a feature
//! that adds a name sorting earlier does not change other scenarios' hashes.
//!
//! [`freeze`]: StableRegistry::freeze

use fl_math::stream_key;
use fl_protocol::StringTables;

struct Entry<V> {
    name: &'static str,
    registrant: &'static str,
    key: u64,
    value: V,
}

/// A table of uniquely named values, sorted by name; frozen once the sim is built.
pub struct StableRegistry<V> {
    table: &'static str,
    /// Always sorted by name, so after [`freeze`](Self::freeze) the index is the dense id.
    entries: Vec<Entry<V>>,
    frozen: bool,
}

impl<V> StableRegistry<V> {
    /// An empty registry exported as string table `table`.
    pub fn new(table: &'static str) -> Self {
        StableRegistry {
            table,
            entries: Vec::new(),
            frozen: false,
        }
    }

    /// The string table this registry exports.
    pub fn table(&self) -> &'static str {
        self.table
    }

    /// Register `value` under `name`, on behalf of feature `registrant`.
    ///
    /// # Panics
    /// If the registry is frozen, or `name` is already registered (the message names both
    /// registrants).
    pub fn register(&mut self, name: &'static str, registrant: &'static str, value: V) {
        assert!(
            !self.frozen,
            "registry '{}' is frozen: '{name}' (from '{registrant}') must be registered in \
             SimFeature::build",
            self.table
        );
        match self.entries.binary_search_by(|e| e.name.cmp(name)) {
            Ok(at) => panic!(
                "'{name}' registered by '{}' and '{registrant}' in registry '{}'",
                self.entries[at].registrant, self.table
            ),
            Err(at) => self.entries.insert(
                at,
                Entry {
                    name,
                    registrant,
                    key: stream_key(name),
                    value,
                },
            ),
        }
    }

    /// Fix the dense ids (name order); every later [`register`](Self::register) panics.
    ///
    /// # Panics
    /// If two names share a 64-bit key, which would make them indistinguishable in the state
    /// hash.
    pub fn freeze(&mut self) {
        let mut keys: Vec<(u64, &'static str)> =
            self.entries.iter().map(|e| (e.key, e.name)).collect();
        keys.sort_unstable();
        if let Some(pair) = keys.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            panic!(
                "'{}' and '{}' share the name key {:016x} in registry '{}'",
                pair[0].1, pair[1].1, pair[0].0, self.table
            );
        }
        self.frozen = true;
    }

    /// Whether [`freeze`](Self::freeze) has run.
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    fn assert_frozen(&self) {
        assert!(
            self.frozen,
            "registry '{}' is not frozen: dense ids exist only after freeze()",
            self.table
        );
    }

    fn entry(&self, id: u32) -> Option<&Entry<V>> {
        self.assert_frozen();
        self.entries.get(usize::try_from(id).ok()?)
    }

    /// The dense id of `name`.
    ///
    /// # Panics
    /// Before [`freeze`](Self::freeze).
    pub fn id(&self, name: &str) -> Option<u32> {
        self.assert_frozen();
        let at = self.entries.binary_search_by(|e| e.name.cmp(name)).ok()?;
        Some(u32::try_from(at).expect("fewer than u32::MAX names"))
    }

    /// The name with dense id `id`.
    ///
    /// # Panics
    /// Before [`freeze`](Self::freeze).
    pub fn name(&self, id: u32) -> Option<&'static str> {
        self.entry(id).map(|e| e.name)
    }

    /// The stable 64-bit key of the name with dense id `id`: `stream_key(name)`.
    ///
    /// # Panics
    /// Before [`freeze`](Self::freeze).
    pub fn name_key(&self, id: u32) -> Option<u64> {
        self.entry(id).map(|e| e.key)
    }

    /// The value registered under `name`. Works before freeze too.
    pub fn get(&self, name: &str) -> Option<&V> {
        let at = self.entries.binary_search_by(|e| e.name.cmp(name)).ok()?;
        Some(&self.entries[at].value)
    }

    /// The value with dense id `id`.
    ///
    /// # Panics
    /// Before [`freeze`](Self::freeze).
    pub fn get_by_id(&self, id: u32) -> Option<&V> {
        self.entry(id).map(|e| &e.value)
    }

    /// `(id, name, value)` in dense-id (name) order.
    ///
    /// # Panics
    /// Before [`freeze`](Self::freeze).
    pub fn iter(&self) -> impl Iterator<Item = (u32, &'static str, &V)> + '_ {
        self.assert_frozen();
        (0u32..)
            .zip(&self.entries)
            .map(|(id, e)| (id, e.name, &e.value))
    }

    /// How many names are registered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no name is registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Write the names, in dense-id order, as string table [`table`](Self::table).
    ///
    /// # Panics
    /// Before [`freeze`](Self::freeze).
    pub fn export(&self, tables: &mut StringTables) {
        self.assert_frozen();
        tables.insert(
            self.table,
            self.entries.iter().map(|e| e.name.to_owned()).collect(),
        );
    }
}
