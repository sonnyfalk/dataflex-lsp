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
    class_lookup_table: HashMap<String, String>,
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

struct SymbolsDiff<'a> {
    added_symbols: Vec<&'a IndexSymbol>,
    removed_symbols: Vec<&'a IndexSymbol>,
}

impl IndexSymbol {
    #[cfg(test)]
    fn class_symbol(&self) -> Option<&ClassSymbol> {
        match self {
            Self::Class(class_symbol) => Some(class_symbol),
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Class(class_symbol) => &class_symbol.name,
        }
    }

    fn is_matching(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Class(class_symbol), Self::Class(other_class_symbol)) => {
                class_symbol.name == other_class_symbol.name
            }
        }
    }
}

impl Index {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            workspace,
            files: HashMap::new(),
            class_lookup_table: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn find_class(&self, name: &str) -> Option<&ClassSymbol> {
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

        class_symbol
    }

    pub fn is_known_class(&self, name: &str) -> bool {
        self.class_lookup_table.get(name).is_some()
    }

    pub fn update_file(&mut self, file_name: &str, index_file: IndexFile) {
        let old_index_file = self.files.insert(file_name.to_string(), index_file);
        self.update_lookup_tables(file_name, old_index_file);
    }

    fn update_lookup_tables(&mut self, file_name: &str, old_index_file: Option<IndexFile>) {
        let Some(new_index_file) = self.files.get(file_name) else {
            // If there's no new index file, just remove all old symbols.
            for symbol in old_index_file.map_or(vec![], |index_file| index_file.symbols) {
                // FIXME: This needs to be updated to support multiple classes with the same name.
                self.class_lookup_table.remove(symbol.name());
            }
            return;
        };
        let Some(old_index_file) = old_index_file else {
            // If there's no old index file, just add all symbols.
            for symbol in &new_index_file.symbols {
                self.class_lookup_table
                    .insert(String::from(symbol.name()), String::from(file_name));
            }
            return;
        };

        // If we have both an old index file and a new one, diff the symbols and update the lookup table accordingly.
        let symbols_diff = old_index_file.diff_symbols(new_index_file);
        for symbol in symbols_diff.removed_symbols {
            // FIXME: This needs to be updated to support multiple classes with the same name.
            self.class_lookup_table.remove(symbol.name());
        }
        for symbol in symbols_diff.added_symbols {
            self.class_lookup_table
                .insert(String::from(symbol.name()), String::from(file_name));
        }
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

    fn diff_symbols<'a>(&'a self, other: &'a Self) -> SymbolsDiff<'a> {
        let existing_symbols = self
            .symbols
            .iter()
            .fold(HashMap::new(), |mut table, symbol| {
                table.insert(symbol.name(), symbol);
                table
            });

        let (added_symbols, removed_symbols) = other.symbols.iter().fold(
            (Vec::new(), existing_symbols),
            |(mut added_symbols, mut existing_symbols), symbol| {
                if let Some(&existing_symbol) = existing_symbols.get(symbol.name()) {
                    if existing_symbol.is_matching(symbol) {
                        existing_symbols.remove(symbol.name());
                    } else {
                        added_symbols.push(symbol);
                    }
                } else {
                    added_symbols.push(symbol);
                }
                return (added_symbols, existing_symbols);
            },
        );

        SymbolsDiff {
            added_symbols,
            removed_symbols: removed_symbols.into_values().collect(),
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

    #[test]
    fn test_diff_symbols_add_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get("test.pkg")
            .unwrap()
            .diff_symbols(new_index.files.get("test.pkg").unwrap());
        assert_eq!(symbols_diff.added_symbols.len(), 1);
        assert_eq!(symbols_diff.removed_symbols.len(), 0);
    }

    #[test]
    fn test_diff_symbols_remove_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get("test.pkg")
            .unwrap()
            .diff_symbols(new_index.files.get("test.pkg").unwrap());
        assert_eq!(symbols_diff.added_symbols.len(), 0);
        assert_eq!(symbols_diff.removed_symbols.len(), 1);
    }

    #[test]
    fn test_diff_symbols_rename_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyRenamedClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get("test.pkg")
            .unwrap()
            .diff_symbols(new_index.files.get("test.pkg").unwrap());
        assert_eq!(symbols_diff.added_symbols.len(), 1);
        assert_eq!(symbols_diff.removed_symbols.len(), 1);
    }

    #[test]
    fn test_class_lookup_table() {
        let index_ref = IndexRef::make_test_index_ref();

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            index_ref.get().class_lookup_table.get("cMyClass"),
            Some(&String::from("test.pkg"))
        );

        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            &PathBuf::from_str("test.pkg").unwrap(),
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
            &PathBuf::from_str("test.pkg").unwrap(),
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
