use super::*;

#[derive(Debug)]
pub struct IndexFile {
    pub path: PathBuf,
    pub dependencies: Vec<String>,
    pub symbols: Vec<IndexSymbol>,
}

impl IndexFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            dependencies: Vec::new(),
            symbols: Vec::new(),
        }
    }
}
