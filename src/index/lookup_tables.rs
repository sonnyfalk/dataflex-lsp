use super::*;
use symbols_diff::SymbolsDiff;

#[derive(Debug)]
pub struct LookupTables {
    class_lookup_table: HashMap<SymbolName, IndexSymbolRef>,
    method_lookup_tables: [MultiMap<SymbolName, IndexSymbolRef>; 3],
    property_lookup_table: MultiMap<SymbolName, IndexSymbolRef>,
}

impl LookupTables {
    pub fn new() -> Self {
        Self {
            class_lookup_table: HashMap::new(),
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

    pub fn method_lookup_table(&self, kind: MethodKind) -> &MultiMap<SymbolName, IndexSymbolRef> {
        match kind {
            MethodKind::Procedure => &self.method_lookup_tables[MethodKind::Procedure as usize],
            MethodKind::Function => &self.method_lookup_tables[MethodKind::Function as usize],
            MethodKind::Set => &self.method_lookup_tables[MethodKind::Set as usize],
        }
    }

    pub fn method_lookup_table_mut(
        &mut self,
        kind: MethodKind,
    ) -> &mut MultiMap<SymbolName, IndexSymbolRef> {
        match kind {
            MethodKind::Procedure => &mut self.method_lookup_tables[MethodKind::Procedure as usize],
            MethodKind::Function => &mut self.method_lookup_tables[MethodKind::Function as usize],
            MethodKind::Set => &mut self.method_lookup_tables[MethodKind::Set as usize],
        }
    }

    pub fn property_lookup_table(&self) -> &MultiMap<SymbolName, IndexSymbolRef> {
        &self.property_lookup_table
    }

    pub fn property_lookup_table_mut(&mut self) -> &mut MultiMap<SymbolName, IndexSymbolRef> {
        &mut self.property_lookup_table
    }

    pub fn is_known_method(&self, name: &SymbolName) -> bool {
        self.method_lookup_tables
            .iter()
            .find_map(|t| t.get(name))
            .is_some()
    }

    pub fn update(&mut self, symbols_diff: SymbolsDiff, file_ref: IndexFileRef) {
        for symbol in symbols_diff.removed_symbols {
            match symbol {
                IndexSymbol::Class(class_symbol) => {
                    for symbol in &class_symbol.methods {
                        if let IndexSymbol::Method(method_symbol) = symbol {
                            if let Some(method_symbols) = self
                                .method_lookup_table_mut(method_symbol.kind)
                                .get_vec_mut(method_symbol.symbol_path.name())
                            {
                                method_symbols
                                    .retain(|s| s.symbol_path != method_symbol.symbol_path);
                                if method_symbols.is_empty() {
                                    self.method_lookup_table_mut(method_symbol.kind)
                                        .remove(method_symbol.symbol_path.name());
                                }
                            }
                        }
                    }
                    for symbol in &class_symbol.properties {
                        if let IndexSymbol::Property(property_symbol) = symbol {
                            if let Some(property_symbols) = self
                                .property_lookup_table_mut()
                                .get_vec_mut(property_symbol.symbol_path.name())
                            {
                                property_symbols
                                    .retain(|s| s.symbol_path != property_symbol.symbol_path);
                                if property_symbols.is_empty() {
                                    self.property_lookup_table_mut()
                                        .remove(property_symbol.symbol_path.name());
                                }
                            }
                        }
                    }
                    // FIXME: This needs to be updated to support multiple classes with the same name.
                    self.class_lookup_table_mut().remove(&class_symbol.name);
                }
                IndexSymbol::Method(method_symbol) => {
                    if let Some(method_symbols) = self
                        .method_lookup_table_mut(method_symbol.kind)
                        .get_vec_mut(method_symbol.symbol_path.name())
                    {
                        method_symbols.retain(|s| s.symbol_path != method_symbol.symbol_path);
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
                        property_symbols.retain(|s| s.symbol_path != property_symbol.symbol_path);
                        if property_symbols.is_empty() {
                            self.property_lookup_table_mut()
                                .remove(property_symbol.symbol_path.name());
                        }
                    }
                }
            }
        }
        for symbol in symbols_diff.added_symbols {
            match symbol {
                IndexSymbol::Class(class_symbol) => {
                    self.class_lookup_table_mut().insert(
                        class_symbol.name.clone(),
                        IndexSymbolRef::new(
                            file_ref.clone(),
                            SymbolPath::new(vec![class_symbol.name.clone()]),
                        ),
                    );
                    for symbol in &class_symbol.methods {
                        if let IndexSymbol::Method(method_symbol) = symbol {
                            self.method_lookup_table_mut(method_symbol.kind).insert(
                                method_symbol.symbol_path.name().clone(),
                                IndexSymbolRef {
                                    file_ref: file_ref.clone(),
                                    symbol_path: method_symbol.symbol_path.clone(),
                                },
                            );
                        }
                    }
                    for symbol in &class_symbol.properties {
                        if let IndexSymbol::Property(property_symbol) = symbol {
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
