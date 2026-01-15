use std::{collections::HashMap, ffi::OsStr, path::PathBuf, str::FromStr};

use multimap::MultiMap;
use streaming_iterator::StreamingIterator;
use strum::EnumString;
use tree_sitter::Point;

mod index_file;
mod index_symbol;
mod indexer;
mod lookup_tables;
mod workspace;

pub use index_symbol::*;

pub use indexer::{Indexer, IndexerConfig, IndexerObserver, IndexerState};
pub use workspace::{DataFlexVersion, WorkspaceInfo};

use index_file::{IndexFile, IndexFileRef};

use crate::index::lookup_tables::LookupTables;

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

impl Index {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            workspace,
            files: HashMap::new(),
            lookup_tables: LookupTables::new(),
        }
    }

    pub fn find_class(&self, name: &SymbolName) -> Option<ClassSymbolSnapshot> {
        if let Some(symbol_ref) = self.lookup_tables.class_lookup_table().get(name) {
            self.find_symbol_ref(symbol_ref)
        } else {
            None
        }
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

    pub fn all_known_properties(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .property_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn is_known_method(&self, name: &SymbolName) -> bool {
        self.lookup_tables.is_known_method(name)
    }

    pub fn all_known_methods(&self, kind: MethodKind) -> Vec<SymbolName> {
        self.lookup_tables
            .method_lookup_table(kind)
            .keys()
            .cloned()
            .collect()
    }

    fn find_symbol_ref<'a, T: IndexSymbolType>(
        &'a self,
        symbol_ref: &IndexSymbolRef,
    ) -> Option<IndexSymbolSnapshot<'a, T>> {
        let Some(index_file) = self.files.get(&symbol_ref.file_ref) else {
            return None;
        };
        let name = symbol_ref.symbol_path.name();

        let symbol = index_file
            .symbols
            .iter()
            .filter(|sym| sym.name() == name)
            .filter_map(|sym| T::from_index_symbol(sym))
            .next();

        symbol.map(|symbol| IndexSymbolSnapshot {
            path: &index_file.path,
            symbol,
        })
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
            format!("{:?}", index_ref.get().find_class(&SymbolName::from("cMyClass"))),
             "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: ClassSymbol { location: Point { row: 0, column: 6 }, name: SymbolName(\"cMyClass\"), methods: [], properties: [] } })"
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
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.class_lookup_table()
                    .get(&SymbolName::from("cMyClass"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\")]) })"
        );

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.class_lookup_table()
                    .get(&SymbolName::from("cMyClass"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.class_lookup_table()
                    .get(&SymbolName::from("cOtherClass"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cOtherClass\")]) })"
        );

        Indexer::index_test_content(
            "Class cMyRenamedClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables
                    .class_lookup_table()
                    .get(&SymbolName::from("cMyClass"))
            ),
            "None"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.class_lookup_table()
                    .get(&SymbolName::from("cMyRenamedClass"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyRenamedClass\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.class_lookup_table()
                    .get(&SymbolName::from("cOtherClass"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cOtherClass\")]) })"
        );
    }

    #[test]
    fn test_method_lookup_table() {
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
                    .lookup_tables.method_lookup_table(MethodKind::Procedure)
                    .get(&SymbolName::from("SayHello"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayHello\")]) })"
        );

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello\n    End_Procedure\n    Procedure SayBye\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Procedure)
                    .get(&SymbolName::from("SayHello"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayHello\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Procedure)
                    .get(&SymbolName::from("SayBye"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayBye\")]) })"
        );

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHelloRenamed\n    End_Procedure\n    Procedure SayBye\n    End_Procedure\n    Function Foo Returns String\n    End_Function\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables
                    .method_lookup_table(MethodKind::Procedure)
                    .get(&SymbolName::from("SayHello"))
            ),
            "None"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Procedure)
                    .get(&SymbolName::from("SayHelloRenamed"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayHelloRenamed\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Procedure)
                    .get(&SymbolName::from("SayBye"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayBye\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Function)
                    .get(&SymbolName::from("Foo"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"Foo\")]) })"
        );
    }

    #[test]
    fn test_property_lookup_table() {
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
                    .lookup_tables.property_lookup_table()
                    .get(&SymbolName::from("piTest"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piTest\")]) })"
        );

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure Construct_Object\n        Property Integer piTest 0\n        Property Integer piOtherTest 0\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.property_lookup_table()
                    .get(&SymbolName::from("piTest"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piTest\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.property_lookup_table()
                    .get(&SymbolName::from("piOtherTest"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piOtherTest\")]) })"
        );

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure Construct_Object\n        Property Integer piRenamedTest 0\n        Property Integer piOtherTest 0\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables
                    .property_lookup_table()
                    .get(&SymbolName::from("piTest"))
            ),
            "None"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.property_lookup_table()
                    .get(&SymbolName::from("piRenamedTest"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piRenamedTest\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.property_lookup_table()
                    .get(&SymbolName::from("piOtherTest"))
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piOtherTest\")]) })"
        );
    }
}
