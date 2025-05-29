use std::{collections::HashMap, ffi::OsStr, path::PathBuf, str::FromStr};

use streaming_iterator::StreamingIterator;
use strum::EnumString;

mod indexer;
mod workspace;

pub use indexer::{Indexer, IndexerConfig};
pub use workspace::{DataFlexVersion, WorkspaceInfo};

#[derive(Debug)]
pub struct Index {
    workspace: WorkspaceInfo,
    files: HashMap<String, IndexFile>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct IndexRef {
    index: std::sync::Arc<std::sync::RwLock<Index>>,
}

#[derive(Debug)]
pub struct IndexFile {
    dependencies: Vec<String>,
    symbols: Vec<IndexSymbol>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum IndexSymbol {
    Class(ClassSymbol),
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ClassSymbol {
    pub name: String,
}

impl IndexSymbol {
    fn class_symbol(&self) -> Option<&ClassSymbol> {
        match self {
            Self::Class(class_symbol) => Some(class_symbol),
            _ => None,
        }
    }
}

impl Index {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            workspace,
            files: HashMap::new(),
        }
    }

    pub fn find_class(&self, name: &str) -> Option<&ClassSymbol> {
        let class_symbol = self
            .files
            .values()
            .map(|f| &f.symbols)
            .flatten()
            .filter_map(IndexSymbol::class_symbol)
            .filter(|c| c.name == name)
            .next();

        class_symbol
    }

    pub fn update_file(&mut self, file_name: &str, index_file: IndexFile) {
        self.files.insert(file_name.to_string(), index_file);
    }
}

impl IndexRef {
    pub fn new(index: Index) -> Self {
        Self {
            index: std::sync::Arc::new(std::sync::RwLock::new(index)),
        }
    }

    pub fn get(&self) -> std::sync::RwLockReadGuard<Index> {
        self.index
            .read()
            .expect("unable to acquire index read lock")
    }

    pub fn get_mut(&self) -> std::sync::RwLockWriteGuard<Index> {
        self.index
            .write()
            .expect("unable to acquire index write lock")
    }
}

impl IndexFile {
    fn new() -> Self {
        Self {
            dependencies: Vec::new(),
            symbols: Vec::new(),
        }
    }
}

#[cfg(test)]
impl Index {
    pub fn make_test_index() -> Self {
        Self::new(WorkspaceInfo::new())
    }
}

#[cfg(test)]
impl IndexRef {
    pub fn make_test_index_ref() -> Self {
        Self::new(Index::make_test_index())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        assert_eq!(
            format!("{:?}", index_ref.get().find_class("cMyClass")),
            "Some(ClassSymbol { name: \"cMyClass\" })"
        );
    }
}
