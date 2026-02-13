use std::{collections::HashMap, ffi::OsStr, path::PathBuf, str::FromStr};

use multimap::MultiMap;
use streaming_iterator::StreamingIterator;
use strum::EnumString;
use tree_sitter::Point;

mod index_file;
mod index_symbol;
mod indexer;
mod lookup_tables;
mod symbols_diff;
mod workspace;

pub use index_symbol::*;

pub use indexer::{Indexer, IndexerConfig, IndexerObserver, IndexerState};
pub use workspace::{DataFlexVersion, WorkspaceInfo};

use index_file::{IndexFile, IndexFileRef};

use lookup_tables::LookupTables;

#[derive(Debug)]
pub struct Index {
    workspace: WorkspaceInfo,
    files: HashMap<IndexFileRef, IndexFile>,
    lookup_tables: LookupTables,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct IndexRef {
    index: std::sync::Arc<std::sync::RwLock<Index>>,
}

pub type ReadableIndexRef<'a> = std::sync::RwLockReadGuard<'a, Index>;
pub type WriteableIndexRef<'a> = std::sync::RwLockWriteGuard<'a, Index>;

impl Index {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            workspace,
            files: HashMap::new(),
            lookup_tables: LookupTables::new(),
        }
    }

    pub fn find_class(&self, name: &SymbolName) -> Option<&IndexSymbolRef> {
        self.lookup_tables.class_lookup_table().get(name)
    }

    pub fn is_known_class(&self, name: &SymbolName) -> bool {
        self.lookup_tables.class_lookup_table().get(name).is_some()
    }

    pub fn all_known_classes(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .class_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn is_known_property(&self, name: &SymbolName) -> bool {
        self.lookup_tables
            .property_lookup_table()
            .get(name)
            .is_some()
    }

    pub fn all_known_properties(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .property_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn find_properties(&self, name: &SymbolName) -> core::slice::Iter<'_, IndexSymbolRef> {
        self.lookup_tables
            .property_lookup_table()
            .get_vec(name)
            .map(|v| v.iter())
            .unwrap_or_default()
    }

    pub fn is_known_method(&self, name: &SymbolName, kind: MethodKind) -> bool {
        self.lookup_tables
            .method_lookup_table(kind)
            .get(name)
            .is_some()
    }

    pub fn all_known_methods(&self, kind: MethodKind) -> Vec<SymbolName> {
        self.lookup_tables
            .method_lookup_table(kind)
            .keys()
            .cloned()
            .collect()
    }

    pub fn find_methods(
        &self,
        name: &SymbolName,
        kind: MethodKind,
    ) -> core::slice::Iter<'_, IndexSymbolRef> {
        self.lookup_tables
            .method_lookup_table(kind)
            .get_vec(name)
            .map(|v| v.iter())
            .unwrap_or_default()
    }

    pub fn symbol_snapshot(
        &self,
        symbol_ref: &IndexSymbolRef,
    ) -> Option<IndexSymbolSnapshot<'_, IndexSymbol>> {
        if let Some(index_file) = self.files.get(&symbol_ref.file_ref) {
            index_file
                .resolve(&symbol_ref.symbol_path)
                .map(|index_symbol| IndexSymbolSnapshot {
                    path: &index_file.path,
                    symbol: index_symbol,
                })
        } else {
            None
        }
    }
}

pub struct IndexSymbolIter<'a> {
    inner: Box<dyn Iterator<Item = IndexSymbolSnapshot<'a, IndexSymbol>> + 'a>,
}

impl<'a> IndexSymbolIter<'a> {
    pub fn new(inner: impl Iterator<Item = IndexSymbolSnapshot<'a, IndexSymbol>> + 'a) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }

    pub fn empty() -> Self {
        Self::new(std::iter::empty())
    }
}

impl<'a> Iterator for IndexSymbolIter<'a> {
    type Item = IndexSymbolSnapshot<'a, IndexSymbol>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl IndexRef {
    pub fn new(index: Index) -> Self {
        Self {
            index: std::sync::Arc::new(std::sync::RwLock::new(index)),
        }
    }

    pub fn get(&self) -> ReadableIndexRef<'_> {
        self.index
            .read()
            .expect("unable to acquire index read lock")
    }

    pub fn get_mut(&self) -> WriteableIndexRef<'_> {
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
            format!("{:?}", index_ref.get().find_class(&SymbolName::from("cMyClass"))),
             "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\")]) })"
        );
    }

    #[test]
    fn test_find_methods() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .find_methods(&SymbolName::from("SayHello"), MethodKind::Procedure).next()
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayHello\")]) })"
        );
    }

    #[test]
    fn test_find_properties() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure Construct_Object\n        Property Integer piTest 0\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .find_properties(&SymbolName::from("piTest"))
                    .next()
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piTest\")]) })"
        );
    }
}
