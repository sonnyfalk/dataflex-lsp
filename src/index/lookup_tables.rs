use super::*;
use symbols_diff::SymbolsDiff;

#[derive(Debug)]
pub struct LookupTables {
    class_lookup_table: HashMap<SymbolName, IndexSymbolRef>,
    object_lookup_table: MultiMap<SymbolName, IndexSymbolRef>,
    method_lookup_tables: [MultiMap<SymbolName, IndexSymbolRef>; 3],
    property_lookup_table: MultiMap<SymbolName, IndexSymbolRef>,
}

impl LookupTables {
    pub fn new() -> Self {
        Self {
            class_lookup_table: HashMap::new(),
            object_lookup_table: MultiMap::new(),
            method_lookup_tables: [MultiMap::new(), MultiMap::new(), MultiMap::new()],
            property_lookup_table: MultiMap::new(),
        }
    }

    pub fn class_lookup_table(&self) -> &HashMap<SymbolName, IndexSymbolRef> {
        &self.class_lookup_table
    }

    pub fn class_lookup_table_mut(&mut self) -> &mut HashMap<SymbolName, IndexSymbolRef> {
        &mut self.class_lookup_table
    }

    #[allow(dead_code)]
    pub fn object_lookup_table(&self) -> &MultiMap<SymbolName, IndexSymbolRef> {
        &self.object_lookup_table
    }

    pub fn object_lookup_table_mut(&mut self) -> &mut MultiMap<SymbolName, IndexSymbolRef> {
        &mut self.object_lookup_table
    }

    pub fn method_lookup_table(&self, kind: MethodKind) -> &MultiMap<SymbolName, IndexSymbolRef> {
        match kind {
            MethodKind::Msg => &self.method_lookup_tables[MethodKind::Msg as usize],
            MethodKind::Get => &self.method_lookup_tables[MethodKind::Get as usize],
            MethodKind::Set => &self.method_lookup_tables[MethodKind::Set as usize],
        }
    }

    pub fn method_lookup_table_mut(
        &mut self,
        kind: MethodKind,
    ) -> &mut MultiMap<SymbolName, IndexSymbolRef> {
        match kind {
            MethodKind::Msg => &mut self.method_lookup_tables[MethodKind::Msg as usize],
            MethodKind::Get => &mut self.method_lookup_tables[MethodKind::Get as usize],
            MethodKind::Set => &mut self.method_lookup_tables[MethodKind::Set as usize],
        }
    }

    pub fn property_lookup_table(&self) -> &MultiMap<SymbolName, IndexSymbolRef> {
        &self.property_lookup_table
    }

    pub fn property_lookup_table_mut(&mut self) -> &mut MultiMap<SymbolName, IndexSymbolRef> {
        &mut self.property_lookup_table
    }

    pub fn update(&mut self, symbols_diff: SymbolsDiff, file_ref: IndexFileRef) {
        self.remove_symols(symbols_diff.removed_symbols.into_iter(), &file_ref);
        self.add_symbols(symbols_diff.added_symbols.into_iter(), &file_ref);
    }

    fn remove_symols<'a>(
        &mut self,
        symbols: impl std::iter::Iterator<Item = &'a IndexSymbol>,
        file_ref: &IndexFileRef,
    ) {
        for symbol in symbols {
            match symbol {
                IndexSymbol::Class(class_symbol) => {
                    self.remove_symols(class_symbol.members.iter(), file_ref);
                    // FIXME: This needs to be updated to support multiple classes with the same name.
                    self.class_lookup_table_mut()
                        .remove(class_symbol.symbol_path.name());
                }
                IndexSymbol::Object(class_symbol) => {
                    self.remove_symols(class_symbol.members.iter(), file_ref);
                    if let Some(object_symbols) = self
                        .object_lookup_table_mut()
                        .get_vec_mut(class_symbol.symbol_path.name())
                    {
                        object_symbols.retain(|s| {
                            s.symbol_path != class_symbol.symbol_path || s.file_ref != *file_ref
                        });
                        if object_symbols.is_empty() {
                            self.object_lookup_table_mut()
                                .remove(class_symbol.symbol_path.name());
                        }
                    }
                }
                IndexSymbol::Method(method_symbol) => {
                    if let Some(method_symbols) = self
                        .method_lookup_table_mut(method_symbol.kind)
                        .get_vec_mut(method_symbol.symbol_path.name())
                    {
                        method_symbols.retain(|s| {
                            s.symbol_path != method_symbol.symbol_path || s.file_ref != *file_ref
                        });
                        if method_symbols.is_empty() {
                            self.method_lookup_table_mut(method_symbol.kind)
                                .remove(method_symbol.symbol_path.name());
                        }
                    }
                }
                IndexSymbol::Property(property_symbol) => {
                    if let Some(property_symbols) = self
                        .property_lookup_table_mut()
                        .get_vec_mut(property_symbol.symbol_path.name())
                    {
                        property_symbols.retain(|s| {
                            s.symbol_path != property_symbol.symbol_path || s.file_ref != *file_ref
                        });
                        if property_symbols.is_empty() {
                            self.property_lookup_table_mut()
                                .remove(property_symbol.symbol_path.name());
                        }
                    }
                }
            }
        }
    }

    fn add_symbols<'a>(
        &mut self,
        symbols: impl std::iter::Iterator<Item = &'a IndexSymbol>,
        file_ref: &IndexFileRef,
    ) {
        for symbol in symbols {
            match symbol {
                IndexSymbol::Class(class_symbol) => {
                    self.class_lookup_table_mut().insert(
                        class_symbol.symbol_path.name().clone(),
                        IndexSymbolRef::new(file_ref.clone(), class_symbol.symbol_path.clone()),
                    );
                    self.add_symbols(class_symbol.members.iter(), file_ref);
                }
                IndexSymbol::Object(class_symbol) => {
                    self.object_lookup_table_mut().insert(
                        class_symbol.symbol_path.name().clone(),
                        IndexSymbolRef {
                            file_ref: file_ref.clone(),
                            symbol_path: class_symbol.symbol_path.clone(),
                        },
                    );
                    self.add_symbols(class_symbol.members.iter(), file_ref);
                }
                IndexSymbol::Method(method_symbol) => {
                    self.method_lookup_table_mut(method_symbol.kind).insert(
                        method_symbol.symbol_path.name().clone(),
                        IndexSymbolRef {
                            file_ref: file_ref.clone(),
                            symbol_path: method_symbol.symbol_path.clone(),
                        },
                    );
                }
                IndexSymbol::Property(property_symbol) => {
                    self.property_lookup_table_mut().insert(
                        property_symbol.symbol_path.name().clone(),
                        IndexSymbolRef {
                            file_ref: file_ref.clone(),
                            symbol_path: property_symbol.symbol_path.clone(),
                        },
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    .get(&"cMyClass".into())
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
                    .get(&"cMyClass".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.class_lookup_table()
                    .get(&"cOtherClass".into())
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
                    .get(&"cMyClass".into())
            ),
            "None"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.class_lookup_table()
                    .get(&"cMyRenamedClass".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyRenamedClass\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.class_lookup_table()
                    .get(&"cOtherClass".into())
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
                    .lookup_tables.method_lookup_table(MethodKind::Msg)
                    .get(&"SayHello".into())
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
                    .lookup_tables.method_lookup_table(MethodKind::Msg)
                    .get(&"SayHello".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayHello\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Msg)
                    .get(&"SayBye".into())
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
                    .method_lookup_table(MethodKind::Msg)
                    .get(&"SayHello".into())
            ),
            "None"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Msg)
                    .get(&"SayHelloRenamed".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayHelloRenamed\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Msg)
                    .get(&"SayBye".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayBye\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.method_lookup_table(MethodKind::Get)
                    .get(&"Foo".into())
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
                    .get(&"piTest".into())
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
                    .get(&"piTest".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piTest\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.property_lookup_table()
                    .get(&"piOtherTest".into())
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
                    .get(&"piTest".into())
            ),
            "None"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.property_lookup_table()
                    .get(&"piRenamedTest".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piRenamedTest\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables.property_lookup_table()
                    .get(&"piOtherTest".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"piOtherTest\")]) })"
        );
    }

    #[test]
    fn test_object_lookup_table() {
        let index_ref = IndexRef::make_test_index_ref();

        Indexer::index_test_content(
            "Object oMyObj is a cBaseClass\nEnd_Object\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables
                    .object_lookup_table()
                    .get(&"oMyObj".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"oMyObj\")]) })"
        );

        Indexer::index_test_content(
            "Object oMyObj is a cBaseClass\n    Object oMyInner is a cBaseClass\n    End_Object\nEnd_Object\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables
                    .object_lookup_table()
                    .get(&"oMyObj".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"oMyObj\")]) })"
        );
        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .lookup_tables
                    .object_lookup_table()
                    .get(&"oMyInner".into())
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath([SymbolName(\"oMyObj\"), SymbolName(\"oMyInner\")]) })"
        );
    }
}
