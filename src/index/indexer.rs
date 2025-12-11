use super::*;

pub struct Indexer {
    index: IndexRef,
    config: IndexerConfig,
    dataflex_version: Option<DataFlexVersion>,
}

#[derive(Debug)]
pub struct IndexerConfig {
    versioned_system_paths: HashMap<DataFlexVersion, Vec<PathBuf>>,
    default_version: DataFlexVersion,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IndexerState {
    Initializing,
    InitialIndexing,
    Inactive,
}

pub trait IndexerObserver {
    fn state_transition(&self, old_state: IndexerState, new_state: IndexerState);
}

impl Indexer {
    pub fn new(workspace: WorkspaceInfo, config: IndexerConfig) -> Self {
        let dataflex_version = workspace.get_dataflex_version().cloned();
        Self {
            index: IndexRef::new(Index::new(workspace)),
            config,
            dataflex_version,
        }
    }

    pub fn get_index(&self) -> &IndexRef {
        &self.index
    }

    pub fn start_indexing<T: IndexerObserver + Send + 'static>(&self, observer: T) {
        let index = self.index.clone();
        let system_paths = self
            .config
            .system_path(self.dataflex_version.as_ref())
            .cloned();
        rayon::spawn(move || {
            observer.state_transition(IndexerState::Initializing, IndexerState::InitialIndexing);
            if let Some(system_paths) = system_paths {
                log::info!("Indexing system paths");
                Self::index_system_paths(&system_paths, &index);
            }
            log::info!("Indexing workspace");
            Self::index_workspace(&index);
            log::info!("Finished indexing: {} files", index.get().files.len());
            log::trace!("{:#?}", index.get());
            observer.state_transition(IndexerState::InitialIndexing, IndexerState::Inactive);
            Self::watch_and_index_changed_files(&index);
        });
    }

    fn index_system_paths(paths: &Vec<PathBuf>, index: &IndexRef) {
        rayon::in_place_scope(|scope| {
            for path in paths {
                if path.is_absolute() {
                    log::trace!("Indexing {:?}", path);
                    Self::index_directory(path, index, &scope);
                }
            }
        });
    }

    fn index_workspace(index: &IndexRef) {
        let root_folder = index.get().workspace.get_root_folder().clone();
        rayon::in_place_scope(|scope| {
            Self::index_directory(&root_folder, index, &scope);
        });
    }

    fn index_directory<'a>(path: &PathBuf, index: &'a IndexRef, scope: &rayon::Scope<'a>) {
        let Some(path_entries) = path.read_dir().ok() else {
            return;
        };
        for path in path_entries.filter_map(|p| Some(p.ok()?.path())) {
            if path.is_dir() {
                Self::index_directory(&path, index, scope);
            } else if Self::should_index_file(&path) {
                Self::index_file(path, index, scope);
            }
        }
    }

    fn index_file<'a>(path: PathBuf, index: &'a IndexRef, scope: &rayon::Scope<'a>) {
        if !path.is_file() || !path.exists() {
            return;
        }
        let Some(content) = std::fs::read(&path).ok() else {
            return;
        };
        scope.spawn(move |_| {
            Self::index_file_content(&content, path, index);
        });
    }

    fn index_file_content(content: &[u8], path: PathBuf, index: &IndexRef) {
        log::trace!("Indexing file content for {:?}", path);
        let mut parser = Self::make_parser();

        let Some(tree) = parser.parse(content, None) else {
            return;
        };

        Self::index_parse_tree(&tree, content, path, index);
    }

    fn index_parse_tree(tree: &tree_sitter::Tree, content: &[u8], path: PathBuf, index: &IndexRef) {
        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            return;
        };
        let file_name = String::from(file_name);

        log::trace!("Indexing file parse tree for {:?}", path);

        let query = tree_sitter::Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            Self::indexer_query(),
        )
        .expect("Error loading indexer query");

        let pattern_index_element_map: Vec<Option<TagsQueryIndexElement>> = (0..query
            .pattern_count())
            .map(|pattern_index| {
                query
                    .property_settings(pattern_index)
                    .iter()
                    .find_map(|p| match p.key.as_ref() {
                        "index.element" => TagsQueryIndexElement::from_str(p.value.as_ref()?).ok(),
                        _ => None,
                    })
            })
            .collect();
        let name_capture_index = query.capture_index_for_name("name").unwrap();
        let mut query_cursor = tree_sitter::QueryCursor::new();
        let matches = query_cursor.matches(&query, tree.root_node(), content);

        let index_file = matches.fold(IndexFile::new(path), |mut index_file, query_match| {
            match pattern_index_element_map[query_match.pattern_index] {
                Some(TagsQueryIndexElement::FileDependency) => {
                    if let Some(file_dependency) = query_match
                        .nodes_for_capture_index(name_capture_index)
                        .next()
                        .map(|node| node.utf8_text(content).ok())
                        .flatten()
                    {
                        index_file
                            .dependencies
                            .push(IndexFileRef::from(file_dependency));
                    }
                }
                Some(TagsQueryIndexElement::ClassDefinition) => {
                    if let Some(name_node) = query_match
                        .nodes_for_capture_index(name_capture_index)
                        .next()
                    {
                        if let Some(name) = name_node.utf8_text(content).ok() {
                            let class_symbol = ClassSymbol {
                                location: name_node.start_position(),
                                name: SymbolName::from(name),
                                methods: Vec::new(),
                            };
                            index_file.symbols.push(IndexSymbol::Class(class_symbol));
                        }
                    }
                }
                Some(TagsQueryIndexElement::MethodProcedureDefinition) => {
                    if let Some(name_node) = query_match
                        .nodes_for_capture_index(name_capture_index)
                        .next()
                    {
                        if let Some(name) = name_node.utf8_text(content).ok() {
                            if let Some(class_symbol) = index_file
                                .symbols
                                .last_mut()
                                .and_then(ClassSymbol::from_index_symbol_mut)
                            {
                                let method_symbol = MethodSymbol {
                                    location: name_node.start_position(),
                                    symbol_path: SymbolPath::new(vec![
                                        class_symbol.name.clone(),
                                        SymbolName::from(name),
                                    ]),
                                    kind: MethodKind::Procedure,
                                };
                                class_symbol
                                    .methods
                                    .push(IndexSymbol::Method(method_symbol));
                            }
                        }
                    }
                }
                Some(TagsQueryIndexElement::MethodFunctionDefinition) => {
                    if let Some(name_node) = query_match
                        .nodes_for_capture_index(name_capture_index)
                        .next()
                    {
                        if let Some(name) = name_node.utf8_text(content).ok() {
                            if let Some(class_symbol) = index_file
                                .symbols
                                .last_mut()
                                .and_then(ClassSymbol::from_index_symbol_mut)
                            {
                                let method_symbol = MethodSymbol {
                                    location: name_node.start_position(),
                                    symbol_path: SymbolPath::new(vec![
                                        class_symbol.name.clone(),
                                        SymbolName::from(name),
                                    ]),
                                    kind: MethodKind::Function,
                                };
                                class_symbol
                                    .methods
                                    .push(IndexSymbol::Method(method_symbol));
                            }
                        }
                    }
                }
                _ => {}
            };
            index_file
        });

        index.get_mut().update_file(file_name, index_file);
    }

    fn watch_and_index_changed_files(_index: &IndexRef) {
        log::trace!("Watching workspace files");
    }

    fn make_parser() -> tree_sitter::Parser {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_dataflex::LANGUAGE.into())
            .expect("Error loading DataFlex grammar");
        parser
    }

    fn should_index_file(path: &PathBuf) -> bool {
        match path.extension().and_then(OsStr::to_str) {
            Some("pkg" | "vw" | "wo" | "sl" | "dd") => true,
            _ => false,
        }
    }

    fn indexer_query() -> &'static str {
        include_str!("indexer.scm")
    }
}

impl IndexerConfig {
    pub fn new() -> Self {
        if let Some(versioned_system_paths) = Self::versioned_system_paths() {
            let default_version = versioned_system_paths
                .iter()
                .map(|(version, _)| version.clone())
                .next()
                .unwrap_or_default();
            Self {
                versioned_system_paths,
                default_version,
            }
        } else {
            Self {
                versioned_system_paths: HashMap::new(),
                default_version: Default::default(),
            }
        }
    }

    pub fn system_path(&self, dataflex_version: Option<&DataFlexVersion>) -> Option<&Vec<PathBuf>> {
        let dataflex_version = dataflex_version.unwrap_or(&self.default_version);
        self.versioned_system_paths
            .get(dataflex_version)
            .or(self.versioned_system_paths.get(&self.default_version))
    }

    #[cfg(target_os = "windows")]
    fn versioned_system_paths() -> Option<HashMap<DataFlexVersion, Vec<PathBuf>>> {
        let reg_key = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey("SOFTWARE\\Data Access Worldwide\\DataFlex")
            .ok()?;

        Some(reg_key.enum_keys().flat_map(Result::ok).fold(
            HashMap::new(),
            |mut result, version: String| {
                let make_path: Option<String> = reg_key
                    .open_subkey(format!("{version}\\DfComp"))
                    .and_then(|k| k.get_value("MakePath"))
                    .ok();
                if let Some(make_path) = make_path {
                    result.insert(
                        DataFlexVersion::from(version),
                        make_path
                            .split(";")
                            .map(str::trim)
                            .map(PathBuf::from)
                            .collect(),
                    );
                }
                result
            },
        ))
    }

    #[cfg(not(target_os = "windows"))]
    fn versioned_system_paths() -> Option<HashMap<DataFlexVersion, Vec<PathBuf>>> {
        None
    }
}

#[derive(EnumString)]
#[strum(serialize_all = "snake_case")]
enum TagsQueryIndexElement {
    FileDependency,
    ClassDefinition,
    MethodProcedureDefinition,
    MethodFunctionDefinition,
}

struct SymbolsDiff<'a> {
    added_symbols: Vec<&'a IndexSymbol>,
    removed_symbols: Vec<&'a IndexSymbol>,
}

impl<'a> SymbolsDiff<'a> {
    fn diff_index_files(
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

impl Index {
    fn update_file(&mut self, file_name: String, index_file: IndexFile) {
        let file_ref = IndexFileRef::from(file_name);
        let old_index_file = self.files.insert(file_ref.clone(), index_file);
        self.update_lookup_tables(&file_ref, old_index_file);
    }

    fn update_lookup_tables(&mut self, file_ref: &IndexFileRef, old_index_file: Option<IndexFile>) {
        let symbols_diff =
            SymbolsDiff::diff_index_files(old_index_file.as_ref(), self.files.get(file_ref));

        for symbol in symbols_diff.removed_symbols {
            match symbol {
                IndexSymbol::Class(class_symbol) => {
                    for symbol in &class_symbol.methods {
                        if let IndexSymbol::Method(method_symbol) = symbol {
                            if let Some(method_symbols) = self
                                .lookup_tables
                                .method_lookup_table_mut(method_symbol.kind)
                                .get_vec_mut(method_symbol.symbol_path.name())
                            {
                                method_symbols
                                    .retain(|s| s.symbol_path != method_symbol.symbol_path);
                                if method_symbols.is_empty() {
                                    self.lookup_tables
                                        .method_lookup_table_mut(method_symbol.kind)
                                        .remove(method_symbol.symbol_path.name());
                                }
                            }
                        }
                    }
                    // FIXME: This needs to be updated to support multiple classes with the same name.
                    self.lookup_tables
                        .class_lookup_table_mut()
                        .remove(&class_symbol.name);
                }
                IndexSymbol::Method(method_symbol) => {
                    if let Some(method_symbols) = self
                        .lookup_tables
                        .method_lookup_table_mut(method_symbol.kind)
                        .get_vec_mut(method_symbol.symbol_path.name())
                    {
                        method_symbols.retain(|s| s.symbol_path != method_symbol.symbol_path);
                        if method_symbols.is_empty() {
                            self.lookup_tables
                                .method_lookup_table_mut(method_symbol.kind)
                                .remove(method_symbol.symbol_path.name());
                        }
                    }
                }
            }
        }
        for symbol in symbols_diff.added_symbols {
            match symbol {
                IndexSymbol::Class(class_symbol) => {
                    self.lookup_tables.class_lookup_table_mut().insert(
                        class_symbol.name.clone(),
                        IndexSymbolRef::new(
                            file_ref.clone(),
                            SymbolPath::new(vec![class_symbol.name.clone()]),
                        ),
                    );
                    for symbol in &class_symbol.methods {
                        if let IndexSymbol::Method(method_symbol) = symbol {
                            self.lookup_tables
                                .method_lookup_table_mut(method_symbol.kind)
                                .insert(
                                    method_symbol.symbol_path.name().clone(),
                                    IndexSymbolRef {
                                        file_ref: file_ref.clone(),
                                        symbol_path: method_symbol.symbol_path.clone(),
                                    },
                                );
                        }
                    }
                }
                IndexSymbol::Method(method_symbol) => {
                    self.lookup_tables
                        .method_lookup_table_mut(method_symbol.kind)
                        .insert(
                            method_symbol.symbol_path.name().clone(),
                            IndexSymbolRef {
                                file_ref: file_ref.clone(),
                                symbol_path: method_symbol.symbol_path.clone(),
                            },
                        );
                }
            }
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
                            let mut inner_diff =
                                diff_symbols(&old_class_symbol.methods, &new_class_symbol.methods);
                            symbols_diff
                                .added_symbols
                                .append(&mut inner_diff.added_symbols);
                            symbols_diff
                                .removed_symbols
                                .append(&mut inner_diff.removed_symbols);
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
impl Indexer {
    pub fn index_test_content(content: &str, path: PathBuf, index: &IndexRef) {
        Self::index_file_content(content.as_bytes(), path, index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_file_dependency() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Use cWebView.pkg\n",
            PathBuf::from_str("test.vw").unwrap(),
            &index_ref,
        );

        assert_eq!(
            index_ref.get().files[&IndexFileRef::from("test.vw")].dependencies,
            [IndexFileRef::from("cWebView.pkg")]
        );
    }

    #[test]
    fn test_index_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        assert_eq!(
            format!("{:?}", index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols),
            "[Class(ClassSymbol { location: Point { row: 0, column: 6 }, name: SymbolName(\"cMyClass\"), methods: [] })]"
        );
    }

    #[test]
    fn test_index_class_procedure_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello\n    End_Procedure\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        assert_eq!(
            format!("{:?}", index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols),
            "[Class(ClassSymbol { location: Point { row: 0, column: 6 }, name: SymbolName(\"cMyClass\"), methods: [Method(MethodSymbol { location: Point { row: 1, column: 14 }, symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayHello\")]), kind: Procedure })] })]"
        );
    }

    #[test]
    fn test_index_class_function_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Function SayHello Returns String\n    End_Function\nEnd_Class\n",
            PathBuf::from_str("test.pkg").unwrap(),
            &index_ref,
        );

        assert_eq!(
            format!("{:?}", index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols),
            "[Class(ClassSymbol { location: Point { row: 0, column: 6 }, name: SymbolName(\"cMyClass\"), methods: [Method(MethodSymbol { location: Point { row: 1, column: 13 }, symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"SayHello\")]), kind: Function })] })]"
        );
    }

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
