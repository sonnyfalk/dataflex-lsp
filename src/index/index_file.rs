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
