use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexFile {
    pub path: PathBuf,
    pub dependencies: Vec<IndexFileRef>,
    pub symbols: Vec<IndexSymbol>,
    pub tables: Option<Box<Vec<DataFlexTable>>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
