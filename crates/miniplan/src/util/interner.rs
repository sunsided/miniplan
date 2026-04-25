use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Default)]
pub struct Interner {
    map: FxHashMap<String, usize>,
    rev: Vec<String>,
}

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, s: &str) -> usize {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = self.rev.len();
        self.map.insert(s.to_owned(), id);
        self.rev.push(s.to_owned());
        id
    }

    pub fn resolve(&self, id: usize) -> &str {
        &self.rev[id]
    }

    pub fn len(&self) -> usize {
        self.rev.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rev.is_empty()
    }
}
