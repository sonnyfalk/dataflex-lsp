use std::{collections::HashMap, ffi::OsStr, path::PathBuf, str::FromStr};

use streaming_iterator::StreamingIterator;
use strum::EnumString;

mod indexer;
mod workspace;

pub use indexer::{Indexer, IndexerConfig};
pub use workspace::WorkspaceInfo;

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
}

impl Index {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            workspace,
            files: HashMap::new(),
        }
    }
}

impl IndexRef {
    pub fn new(index: Index) -> Self {
        Self {
            index: std::sync::Arc::new(std::sync::RwLock::new(index)),
        }
    }

    #[allow(dead_code)]
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
