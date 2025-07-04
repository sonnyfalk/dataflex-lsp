use std::{collections::HashMap, ffi::OsStr, path::PathBuf, str::FromStr};

use streaming_iterator::StreamingIterator;
use strum::EnumString;

mod index_file;
mod index_symbol;
mod indexer;
mod workspace;

pub use index_symbol::{ClassSymbol, ClassSymbolSnapshot, IndexSymbol, MethodKind, MethodSymbol};
pub use indexer::{Indexer, IndexerConfig, IndexerObserver, IndexerState};
pub use workspace::{DataFlexVersion, WorkspaceInfo};

use index_file::IndexFile;
use tree_sitter::Point;

#[derive(Debug)]
pub struct Index {
    workspace: WorkspaceInfo,
    files: HashMap<String, IndexFile>,
    class_lookup_table: HashMap<String, String>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct IndexRef {
    index: std::sync::Arc<std::sync::RwLock<Index>>,
}

impl Index {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            workspace,
            files: HashMap::new(),
            class_lookup_table: HashMap::new(),
        }
    }

    pub fn find_class(&self, name: &str) -> Option<ClassSymbolSnapshot> {
        let Some(file) = self.class_lookup_table.get(name) else {
            return None;
        };

        let Some(index_file) = self.files.get(file) else {
            return None;
        };

        let class_symbol = index_file
            .symbols
            .iter()
            .filter_map(IndexSymbol::class_symbol)
            .filter(|c| c.name == name)
            .next();

        if let Some(class_symbol) = class_symbol {
            Some(ClassSymbolSnapshot {
                path: &index_file.path,
                symbol: class_symbol,
            })
        } else {
            None
        }
    }

    pub fn is_known_class(&self, name: &str) -> bool {
        self.class_lookup_table.get(name).is_some()
    }

    pub fn all_known_classes(&self) -> Vec<String> {
        self.class_lookup_table.keys().cloned().collect()
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
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        assert_eq!(
            format!("{:?}", index_ref.get().find_class("cMyClass")),
             "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: ClassSymbol { location: Point { row: 0, column: 6 }, name: \"cMyClass\", methods: [] } })"
        );
    }

    #[test]
    fn test_class_lookup_table() {
        let index_ref = IndexRef::make_test_index_ref();

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            index_ref.get().class_lookup_table.get("cMyClass"),
            Some(&String::from("test.pkg"))
        );

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            index_ref.get().class_lookup_table.get("cMyClass"),
            Some(&String::from("test.pkg"))
        );
        assert_eq!(
            index_ref.get().class_lookup_table.get("cOtherClass"),
            Some(&String::from("test.pkg"))
        );

        Indexer::index_test_content(
            "Class cMyRenamedClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(index_ref.get().class_lookup_table.get("cMyClass"), None);
        assert_eq!(
            index_ref.get().class_lookup_table.get("cMyRenamedClass"),
            Some(&String::from("test.pkg"))
        );
        assert_eq!(
            index_ref.get().class_lookup_table.get("cOtherClass"),
            Some(&String::from("test.pkg"))
        );
    }
}
