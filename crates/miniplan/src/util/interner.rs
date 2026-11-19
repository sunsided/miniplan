//! A string interner mapping names to dense integer ids.

use rustc_hash::FxHashMap;

/// Assigns each distinct string a dense id (`0, 1, 2, ...` in first-seen
/// order) and maps ids back to strings.
///
/// Ids are stable for the interner's lifetime; nothing is ever removed.
#[derive(Debug, Clone, Default)]
#[expect(
    dead_code,
    reason = "currently unused by the crate; kept as a ready helper rather than removed in a \
              lint-only change"
)]
pub struct Interner {
    /// String to id lookup; the inverse of `rev`.
    map: FxHashMap<String, usize>,
    /// Interned strings indexed by id.
    rev: Vec<String>,
}

#[expect(
    dead_code,
    reason = "currently unused by the crate; kept as a ready helper rather than removed in a \
              lint-only change"
)]
impl Interner {
    /// Creates an empty interner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the id of `s`, assigning the next free id if it is new.
    pub fn intern(&mut self, s: &str) -> usize {
        if let Some(&id) = self.map.get(s) {
            return id;
        }

        let id = self.rev.len();
        self.map.insert(s.to_owned(), id);
        self.rev.push(s.to_owned());
        id
    }

    /// Returns the string interned under `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` was not returned by [`Interner::intern`] on this
    /// interner.
    pub fn resolve(&self, id: usize) -> &str {
        &self.rev[id]
    }

    /// Number of distinct strings interned so far.
    pub fn len(&self) -> usize {
        self.rev.len()
    }

    /// Whether no string has been interned yet.
    pub fn is_empty(&self) -> bool {
        self.rev.is_empty()
    }
}
