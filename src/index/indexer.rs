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

    pub fn start_indexing(&self) {
        let index = self.index.clone();
        let system_paths = self
            .config
            .system_path(self.dataflex_version.as_ref())
            .cloned();
        rayon::spawn(move || {
            if let Some(system_paths) = system_paths {
                log::info!("Indexing system paths");
                Self::index_system_paths(&system_paths, &index);
            }
            log::info!("Indexing workspace");
            Self::index_workspace(&index);
            log::info!("Finished indexing: {} files", index.get().files.len());
            log::trace!("{:#?}", index.get());
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
            tree_sitter_dataflex::TAGS_QUERY,
        )
        .expect("Error loading TAGS_QUERY");

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
                        index_file.dependencies.push(file_dependency.to_string());
                    }
                }
                Some(TagsQueryIndexElement::ClassDefinition) => {
                    if let Some(name) = query_match
                        .nodes_for_capture_index(name_capture_index)
                        .next()
                        .map(|node| node.utf8_text(content).ok())
                        .flatten()
                    {
                        let class_symbol = ClassSymbol {
                            name: String::from(name),
                        };
                        index_file.symbols.push(IndexSymbol::Class(class_symbol));
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
            index_ref.get().files["test.vw"].dependencies,
            ["cWebView.pkg"]
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
            format!("{:?}", index_ref.get().files["test.pkg"].symbols),
            "[Class(ClassSymbol { name: \"cMyClass\" })]"
        );
    }
}
