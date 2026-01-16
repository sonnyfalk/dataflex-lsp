use super::*;

pub struct SymbolsDiff<'a> {
    pub added_symbols: Vec<&'a IndexSymbol>,
    pub removed_symbols: Vec<&'a IndexSymbol>,
}

impl<'a> SymbolsDiff<'a> {
    pub fn diff_index_files(
        old_index_file: Option<&'a IndexFile>,
        new_index_file: Option<&'a IndexFile>,
    ) -> SymbolsDiff<'a> {
        match (old_index_file, new_index_file) {
            (Some(old_index_file), Some(new_index_file)) => {
                // If we have both an old index file and a new one, diff the symbols.
                old_index_file.diff_symbols(new_index_file)
            }
            (Some(old_index_file), None) => {
                // If there's no new index file, just remove all old symbols.
                SymbolsDiff {
                    added_symbols: vec![],
                    removed_symbols: old_index_file.symbols.iter().collect(),
                }
            }
            (None, Some(new_index_file)) => {
                // If there's no old index file, just add all new symbols.
                SymbolsDiff {
                    added_symbols: new_index_file.symbols.iter().collect(),
                    removed_symbols: vec![],
                }
            }
            (None, None) => SymbolsDiff {
                added_symbols: vec![],
                removed_symbols: vec![],
            },
        }
    }
}

impl IndexFile {
    fn diff_symbols<'a>(&'a self, other: &'a Self) -> SymbolsDiff<'a> {
        diff_symbols(&self.symbols, &other.symbols)
    }
}

fn diff_symbols<'a>(
    old_symbols: &'a Vec<IndexSymbol>,
    new_symbols: &'a Vec<IndexSymbol>,
) -> SymbolsDiff<'a> {
    let existing_symbols = old_symbols
        .iter()
        .fold(HashMap::new(), |mut table, symbol| {
            table.insert(symbol.name(), symbol);
            table
        });

    let (mut symbols_diff, removed_symbols) = new_symbols.iter().fold(
        (
            SymbolsDiff {
                added_symbols: vec![],
                removed_symbols: vec![],
            },
            existing_symbols,
        ),
        |(mut symbols_diff, mut existing_symbols), symbol| {
            if let Some(&existing_symbol) = existing_symbols.get(symbol.name()) {
                if existing_symbol.is_matching(symbol) {
                    match (existing_symbol, symbol) {
                        (
                            IndexSymbol::Class(old_class_symbol),
                            IndexSymbol::Class(new_class_symbol),
                        ) => {
                            let mut method_diff =
                                diff_symbols(&old_class_symbol.methods, &new_class_symbol.methods);
                            symbols_diff
                                .added_symbols
                                .append(&mut method_diff.added_symbols);
                            symbols_diff
                                .removed_symbols
                                .append(&mut method_diff.removed_symbols);

                            let mut property_diff = diff_symbols(
                                &old_class_symbol.properties,
                                &new_class_symbol.properties,
                            );
                            symbols_diff
                                .added_symbols
                                .append(&mut property_diff.added_symbols);
                            symbols_diff
                                .removed_symbols
                                .append(&mut property_diff.removed_symbols);
                        }
                        _ => {}
                    }
                    existing_symbols.remove(symbol.name());
                } else {
                    symbols_diff.added_symbols.push(symbol);
                }
            } else {
                symbols_diff.added_symbols.push(symbol);
            }
            (symbols_diff, existing_symbols)
        },
    );
    symbols_diff
        .removed_symbols
        .append(&mut removed_symbols.into_values().collect());

    symbols_diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_symbols_add_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get(&IndexFileRef::from("test.pkg"))
            .unwrap()
            .diff_symbols(
                new_index
                    .files
                    .get(&IndexFileRef::from("test.pkg"))
                    .unwrap(),
            );
        assert_eq!(symbols_diff.added_symbols.len(), 1);
        assert_eq!(symbols_diff.removed_symbols.len(), 0);
    }

    #[test]
    fn test_diff_symbols_remove_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n\nClass cOtherClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get(&IndexFileRef::from("test.pkg"))
            .unwrap()
            .diff_symbols(
                new_index
                    .files
                    .get(&IndexFileRef::from("test.pkg"))
                    .unwrap(),
            );
        assert_eq!(symbols_diff.added_symbols.len(), 0);
        assert_eq!(symbols_diff.removed_symbols.len(), 1);
    }

    #[test]
    fn test_diff_symbols_rename_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyRenamedClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get(&IndexFileRef::from("test.pkg"))
            .unwrap()
            .diff_symbols(
                new_index
                    .files
                    .get(&IndexFileRef::from("test.pkg"))
                    .unwrap(),
            );
        assert_eq!(symbols_diff.added_symbols.len(), 1);
        assert_eq!(symbols_diff.removed_symbols.len(), 1);
    }

    #[test]
    fn test_diff_symbols_add_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get(&IndexFileRef::from("test.pkg"))
            .unwrap()
            .diff_symbols(
                new_index
                    .files
                    .get(&IndexFileRef::from("test.pkg"))
                    .unwrap(),
            );
        assert_eq!(symbols_diff.added_symbols.len(), 1);
        assert_eq!(symbols_diff.removed_symbols.len(), 0);
    }

    #[test]
    fn test_diff_symbols_remove_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get(&IndexFileRef::from("test.pkg"))
            .unwrap()
            .diff_symbols(
                new_index
                    .files
                    .get(&IndexFileRef::from("test.pkg"))
                    .unwrap(),
            );
        assert_eq!(symbols_diff.added_symbols.len(), 0);
        assert_eq!(symbols_diff.removed_symbols.len(), 1);
    }

    #[test]
    fn test_diff_symbols_rename_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        let new_index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayBye\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &new_index_ref,
        );

        let orig_index = index_ref.get();
        let new_index = new_index_ref.get();
        let symbols_diff = orig_index
            .files
            .get(&IndexFileRef::from("test.pkg"))
            .unwrap()
            .diff_symbols(
                new_index
                    .files
                    .get(&IndexFileRef::from("test.pkg"))
                    .unwrap(),
            );
        assert_eq!(symbols_diff.added_symbols.len(), 1);
        assert_eq!(symbols_diff.removed_symbols.len(), 1);
    }
}
