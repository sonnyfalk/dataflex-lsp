use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexFile {
    pub path: PathBuf,
    pub dependencies: Vec<IndexFileRef>,
    pub symbols: Vec<IndexSymbol>,
    pub tables: Option<Box<Vec<DataFlexTable>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexFileRef(std::ffi::OsString);

impl IndexFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            dependencies: Vec::new(),
            symbols: Vec::new(),
            tables: None,
        }
    }

    pub fn child(&self, name: &SymbolName) -> Option<&IndexSymbol> {
        self.symbols.iter().find(|s| s.name() == name)
    }

    pub fn resolve(&self, path: &SymbolPath) -> Option<&IndexSymbol> {
        let mut sym_path_it = path.as_slice().iter();
        if let Some(name) = sym_path_it.next() {
            self.child(name).map(|s| s.resolve(sym_path_it)).flatten()
        } else {
            None
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DataFlexTable {
    pub name: SymbolName,
    pub columns: Vec<SymbolName>,
}

impl From<&PathBuf> for IndexFileRef {
    fn from(value: &PathBuf) -> Self {
        Self(value.file_name().unwrap_or_default().into())
    }
}

impl From<&str> for IndexFileRef {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl From<String> for IndexFileRef {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl TryFrom<IndexFileRef> for String {
    type Error = IndexFileRef;

    fn try_from(value: IndexFileRef) -> Result<Self, Self::Error> {
        value.0.into_string().map_err(IndexFileRef)
    }
}

impl PartialEq for IndexFileRef {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for IndexFileRef {}

impl std::hash::Hash for IndexFileRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}
