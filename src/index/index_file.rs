use super::*;

#[derive(Debug)]
pub struct IndexFile {
    pub path: PathBuf,
    pub dependencies: Vec<IndexFileRef>,
    pub symbols: Vec<IndexSymbol>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IndexFileRef(String);

impl IndexFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            dependencies: Vec::new(),
            symbols: Vec::new(),
        }
    }

    pub fn child(&self, name: &SymbolName) -> Option<&IndexSymbol> {
        self.symbols.iter().find(|s| s.name() == name)
    }

    pub fn resolve(&self, path: &SymbolPath) -> Option<&IndexSymbol> {
        let mut sym_path_it = path.components();
        if let Some(name) = sym_path_it.next() {
            self.child(name).map(|s| s.resolve(sym_path_it)).flatten()
        } else {
            None
        }
    }
}

impl From<String> for IndexFileRef {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for IndexFileRef {
    fn from(value: &str) -> Self {
        Self::from(String::from(value))
    }
}
