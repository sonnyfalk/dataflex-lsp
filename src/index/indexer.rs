use std::sync::{OnceLock, mpsc};

use crate::dataflex_parser::DataFlexTreeParser;

use super::*;
use symbols_diff::SymbolsDiff;

pub struct Indexer {
    index: IndexRef,
    config: IndexerConfig,
    dataflex_version: Option<DataFlexVersion>,
    channel: OnceLock<mpsc::Sender<IndexerMessage>>,
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
    Stopped,
}

#[derive(Debug)]
enum IndexerMessage {
    IndexModifiedFileBuffer(PathBuf, tree_sitter::Tree, String),
    StopIndexing,
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
            channel: OnceLock::new(),
        }
    }

    pub fn get_index(&self) -> &IndexRef {
        &self.index
    }

    pub fn start_indexing<T: IndexerObserver + Send + 'static>(&self, observer: T) {
        let (sender, receiver) = mpsc::channel();
        if self.channel.set(sender).is_err() {
            log::error!("Indexer::start_indexing() should only be called once");
            return;
        }

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
            Self::watch_and_index_changed_files(&index, receiver);
            observer.state_transition(IndexerState::Inactive, IndexerState::Stopped);
            log::info!("Indexer exiting");
        });
    }

    pub fn stop_indexing(&self) {
        let Some(channel) = self.channel.get() else {
            log::error!(
                "Indexer::stop_indexing() cannot be called before indexer is started with Indexer::start_indexing()"
            );
            return;
        };
        _ = channel.send(IndexerMessage::StopIndexing);
    }

    pub fn index_modified_file_buffer(
        &self,
        path: PathBuf,
        tree: tree_sitter::Tree,
        content: String,
    ) {
        let Some(channel) = self.channel.get() else {
            log::error!(
                "Indexer::index_modified_file_buffer() cannot be called before indexer is started with Indexer::start_indexing()"
            );
            return;
        };
        _ = channel.send(IndexerMessage::IndexModifiedFileBuffer(path, tree, content));
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
        let mut parser = DataFlexTreeParser::new();

        let Some(tree) = parser.parse(content, None) else {
            return;
        };

        let index_file = Self::index_parse_tree(&tree, content, path);
        index.get_mut().update_file(index_file);
    }

    fn index_parse_tree(tree: &tree_sitter::Tree, content: &[u8], path: PathBuf) -> IndexFile {
        log::trace!("Indexing file parse tree for {:?}", path);

        let query = tree_sitter::Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            Self::indexer_query(),
        )
        .expect("Error loading indexer query");

        let pattern_index_element_map: Vec<Option<TagsQueryIndexElement>> =
            (0..query.pattern_count())
                .map(|pattern_index| {
                    query.property_settings(pattern_index).iter().find_map(|p| {
                        match p.key.as_ref() {
                            "index.element" => p.value.as_ref()?.parse().ok(),
                            _ => None,
                        }
                    })
                })
                .collect();
        let name_capture_index = query.capture_index_for_name("name").unwrap();
        let superclass_capture_index = query.capture_index_for_name("superclass").unwrap();
        let type_capture_index = query.capture_index_for_name("type").unwrap();
        let array_capture_index = query.capture_index_for_name("array").unwrap();
        let value_ref_capture_index = query.capture_index_for_name("value_reference").unwrap();
        let name_ref_capture_index = query.capture_index_for_name("name_reference").unwrap();
        let arg_ref_capture_index = query.capture_index_for_name("arg_reference").unwrap();
        let parameter_capture_index = query.capture_index_for_name("parameter").unwrap();
        let return_type_capture_index = query.capture_index_for_name("return_type").unwrap();
        let element_node_capture_index = query.capture_index_for_name("element_node").unwrap();
        let mut query_cursor = tree_sitter::QueryCursor::new();
        let matches = query_cursor.matches(&query, tree.root_node(), content);

        let (index_file, _) = matches.fold(
            (IndexFile::new(path), Vec::<IndexSymbol>::new()),
            |(mut index_file, mut stack), query_match| {
                let element_node = query_match
                    .nodes_for_capture_index(element_node_capture_index)
                    .next();
                let element_range = element_node.map(|n| SourceRange::from(n.range()));

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
                            && let Some(name) = name_node.utf8_text(content).ok()
                        {
                            let superclass = query_match
                                .nodes_for_capture_index(superclass_capture_index)
                                .next()
                                .and_then(|n| n.utf8_text(content).ok())
                                .unwrap_or_default();
                            let class_symbol = ClassSymbol {
                                location: name_node.start_position().into(),
                                range: element_range.unwrap_or_else(|| name_node.range().into()),
                                symbol_path: SymbolPath::with_name(name),
                                superclass: superclass.into(),
                                mixins: Vec::new(),
                                members: Vec::new(),
                                metadata: element_node
                                    .as_ref()
                                    .and_then(|symbol_node| {
                                        MetadataTagSet::associated_metadata_tag_sets(
                                            symbol_node,
                                            content,
                                        )
                                    })
                                    .unwrap_or_default(),
                            };
                            stack.push(IndexSymbol::Class(class_symbol));
                        }
                    }
                    Some(TagsQueryIndexElement::ObjectDefinition) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                        {
                            let superclass = query_match
                                .nodes_for_capture_index(superclass_capture_index)
                                .next()
                                .and_then(|n| n.utf8_text(content).ok())
                                .unwrap_or_default();
                            let parent = stack.last().and_then(ClassSymbol::from_index_symbol);
                            let class_symbol = ClassSymbol {
                                location: name_node.start_position().into(),
                                range: element_range.unwrap_or_else(|| name_node.range().into()),
                                symbol_path: parent
                                    .map(|parent| {
                                        SymbolPath::with_parent_and_name(&parent.symbol_path, name)
                                    })
                                    .unwrap_or_else(|| SymbolPath::with_name(name)),
                                superclass: superclass.into(),
                                mixins: Vec::new(),
                                members: Vec::new(),
                                metadata: Vec::new(),
                            };
                            stack.push(IndexSymbol::Object(class_symbol));
                        }
                    }
                    Some(TagsQueryIndexElement::StructDeclaration) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                        {
                            let struct_symbol = StructSymbol {
                                location: name_node.start_position().into(),
                                range: element_range.unwrap_or_else(|| name_node.range().into()),
                                symbol_path: SymbolPath::with_name(name),
                                members: Vec::new(),
                            };
                            stack.push(IndexSymbol::Struct(struct_symbol));
                        }
                    }
                    Some(TagsQueryIndexElement::StructMember) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                            && let Some(struct_symbol) = stack
                                .last_mut()
                                .and_then(StructSymbol::from_index_symbol_mut)
                        {
                            let type_name = query_match
                                .nodes_for_capture_index(type_capture_index)
                                .next()
                                .and_then(|n| n.utf8_text(content).ok())
                                .unwrap_or_default();
                            let array_dimension_count = query_match
                                .nodes_for_capture_index(array_capture_index)
                                .count();
                            let variable_type = if array_dimension_count == 0 {
                                DataFlexDataType::Simple(type_name.into())
                            } else {
                                DataFlexDataType::Array(type_name.into(), array_dimension_count)
                            };
                            let variable_symbol = VariableSymbol {
                                location: name_node.start_position().into(),
                                range: element_range.unwrap_or_else(|| name_node.range().into()),
                                symbol_path: SymbolPath::with_parent_and_name(
                                    &struct_symbol.symbol_path,
                                    name,
                                ),
                                data_type: variable_type,
                                metadata: Vec::new(),
                            };
                            struct_symbol
                                .members
                                .push(IndexSymbol::Variable(variable_symbol));
                        }
                    }
                    Some(TagsQueryIndexElement::MethodProcedureDefinition) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                            && let Some(class_symbol) = stack
                                .last_mut()
                                .and_then(ClassSymbol::from_index_symbol_mut)
                        {
                            let parameters = query_match
                                .nodes_for_capture_index(parameter_capture_index)
                                .filter_map(|parameter_node| {
                                    let name = SymbolName::from(
                                        parameter_node
                                            .child_by_field_name("name")
                                            .and_then(|n| n.utf8_text(content).ok())?,
                                    );
                                    let data_type =
                                        parameter_node.child_by_field_name("type").and_then(
                                            |n| DataFlexDataType::with_typedecl_node(n, content),
                                        )?;
                                    Some((name, data_type))
                                })
                                .collect();

                            let method_kind = if name_node.prev_sibling().is_some_and(|n| {
                                n.utf8_text(content)
                                    .is_ok_and(|text| text.eq_ignore_ascii_case("set"))
                            }) {
                                MethodKind::Set
                            } else {
                                MethodKind::Msg
                            };
                            let method_symbol = MethodSymbol {
                                location: name_node.start_position().into(),
                                range: element_range.unwrap_or_else(|| name_node.range().into()),
                                symbol_path: SymbolPath::with_parent_and_name(
                                    &class_symbol.symbol_path,
                                    name,
                                ),
                                kind: method_kind,
                                parameters: parameters,
                                return_type: None,
                                metadata: element_node
                                    .as_ref()
                                    .and_then(|symbol_node| {
                                        MetadataTagSet::associated_metadata_tag_sets(
                                            symbol_node,
                                            content,
                                        )
                                    })
                                    .unwrap_or_default(),
                            };
                            class_symbol
                                .members
                                .push(IndexSymbol::Method(method_symbol));
                        }
                    }
                    Some(TagsQueryIndexElement::MethodFunctionDefinition) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                            && let Some(class_symbol) = stack
                                .last_mut()
                                .and_then(ClassSymbol::from_index_symbol_mut)
                        {
                            let parameters = query_match
                                .nodes_for_capture_index(parameter_capture_index)
                                .filter_map(|parameter_node| {
                                    let name = SymbolName::from(
                                        parameter_node
                                            .child_by_field_name("name")
                                            .and_then(|n| n.utf8_text(content).ok())?,
                                    );
                                    let data_type =
                                        parameter_node.child_by_field_name("type").and_then(
                                            |n| DataFlexDataType::with_typedecl_node(n, content),
                                        )?;
                                    Some((name, data_type))
                                })
                                .collect();

                            let return_type = query_match
                                .nodes_for_capture_index(return_type_capture_index)
                                .filter_map(|n| DataFlexDataType::with_typedecl_node(n, content))
                                .next();

                            let method_symbol = MethodSymbol {
                                location: name_node.start_position().into(),
                                range: element_range.unwrap_or_else(|| name_node.range().into()),
                                symbol_path: SymbolPath::with_parent_and_name(
                                    &class_symbol.symbol_path,
                                    name,
                                ),
                                kind: MethodKind::Get,
                                parameters: parameters,
                                return_type: return_type,
                                metadata: element_node
                                    .as_ref()
                                    .and_then(|symbol_node| {
                                        MetadataTagSet::associated_metadata_tag_sets(
                                            symbol_node,
                                            content,
                                        )
                                    })
                                    .unwrap_or_default(),
                            };
                            class_symbol
                                .members
                                .push(IndexSymbol::Method(method_symbol));
                        }
                    }
                    Some(TagsQueryIndexElement::PropertyDefinition) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                            && let Some(class_symbol) = stack
                                .last_mut()
                                .and_then(ClassSymbol::from_index_symbol_mut)
                        {
                            let type_name = query_match
                                .nodes_for_capture_index(type_capture_index)
                                .next()
                                .and_then(|n| n.utf8_text(content).ok())
                                .unwrap_or_default();
                            let array_dimension_count = query_match
                                .nodes_for_capture_index(array_capture_index)
                                .count();
                            let variable_type = if array_dimension_count == 0 {
                                DataFlexDataType::Simple(type_name.into())
                            } else {
                                DataFlexDataType::Array(type_name.into(), array_dimension_count)
                            };
                            let variable_symbol = VariableSymbol {
                                location: name_node.start_position().into(),
                                range: element_range.unwrap_or_else(|| name_node.range().into()),
                                symbol_path: SymbolPath::with_parent_and_name(
                                    &class_symbol.symbol_path,
                                    name,
                                ),
                                data_type: variable_type,
                                metadata: element_node
                                    .as_ref()
                                    .and_then(|symbol_node| {
                                        MetadataTagSet::associated_metadata_tag_sets(
                                            symbol_node,
                                            content,
                                        )
                                    })
                                    .unwrap_or_default(),
                            };
                            class_symbol
                                .members
                                .push(IndexSymbol::Property(variable_symbol));
                        }
                    }
                    Some(TagsQueryIndexElement::GlobalVariableDeclaration) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                        {
                            let type_name = query_match
                                .nodes_for_capture_index(type_capture_index)
                                .next()
                                .and_then(|n| n.utf8_text(content).ok())
                                .unwrap_or_default();
                            let array_dimension_count = query_match
                                .nodes_for_capture_index(array_capture_index)
                                .count();
                            let variable_type = if array_dimension_count == 0 {
                                DataFlexDataType::Simple(type_name.into())
                            } else {
                                DataFlexDataType::Array(type_name.into(), array_dimension_count)
                            };
                            let variable_symbol = VariableSymbol {
                                location: name_node.start_position().into(),
                                range: element_range.unwrap_or_else(|| name_node.range().into()),
                                symbol_path: SymbolPath::with_name(name),
                                data_type: variable_type,
                                metadata: Vec::new(),
                            };
                            index_file
                                .symbols
                                .push(IndexSymbol::Variable(variable_symbol));
                        }
                    }
                    Some(TagsQueryIndexElement::AliasDefinition) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                        {
                            if let Some(arg_ref) = query_match
                                .nodes_for_capture_index(arg_ref_capture_index)
                                .next()
                                .and_then(|n| n.utf8_text(content).ok())
                                && (arg_ref.starts_with("|F") || arg_ref.starts_with("|f"))
                            {
                                if let Some((table_name, column_name)) =
                                    name.split_once('.').map(|parts| {
                                        (SymbolName::from(parts.0), SymbolName::from(parts.1))
                                    })
                                {
                                    let tables = index_file.tables.get_or_insert_default();
                                    if let Some(table) =
                                        tables.iter_mut().find(|t| t.name == table_name)
                                    {
                                        table.columns.push(column_name);
                                    } else {
                                        tables.push(DataFlexTable {
                                            name: table_name,
                                            columns: vec!["File_Number".into(), column_name],
                                        });
                                    }
                                }
                            } else {
                                let value = if let Some(value) = query_match
                                    .nodes_for_capture_index(value_ref_capture_index)
                                    .next()
                                    .or_else(|| {
                                        query_match
                                            .nodes_for_capture_index(arg_ref_capture_index)
                                            .next()
                                    })
                                    .and_then(|n| n.utf8_text(content).ok())
                                {
                                    ValueReference::Value(value.into())
                                } else if let Some(name_ref) = query_match
                                    .nodes_for_capture_index(name_ref_capture_index)
                                    .next()
                                    .and_then(|n| n.utf8_text(content).ok())
                                {
                                    ValueReference::Symbol(name_ref.into())
                                } else {
                                    //FIXME: This should increment by one for enum list and use "1" otherwise
                                    ValueReference::Value(String::new())
                                };
                                let alias_symbol = AliasSymbol {
                                    location: name_node.start_position().into(),
                                    range: element_range
                                        .unwrap_or_else(|| name_node.range().into()),
                                    symbol_path: SymbolPath::with_name(name),
                                    alias: value,
                                };
                                index_file.symbols.push(IndexSymbol::Alias(alias_symbol));
                            }
                        }
                    }
                    Some(TagsQueryIndexElement::MixinClass) => {
                        if let Some(name_node) = query_match
                            .nodes_for_capture_index(name_capture_index)
                            .next()
                            && let Some(name) = name_node.utf8_text(content).ok()
                            && let Some(class_symbol) = stack
                                .last_mut()
                                .and_then(ClassSymbol::from_index_symbol_mut)
                        {
                            class_symbol.mixins.push(name.into());
                        }
                    }
                    Some(TagsQueryIndexElement::PopStackSymbol) => {
                        if let Some(symbol) = stack.pop() {
                            match stack.last_mut() {
                                Some(IndexSymbol::Object(class_symbol)) => {
                                    class_symbol.members.push(symbol);
                                }
                                Some(IndexSymbol::Class(class_symbol)) => {
                                    class_symbol.members.push(symbol);
                                }
                                Some(IndexSymbol::Struct(_))
                                | Some(IndexSymbol::Method(_))
                                | Some(IndexSymbol::Property(_))
                                | Some(IndexSymbol::Variable(_))
                                | Some(IndexSymbol::Alias(_))
                                | None => {
                                    index_file.symbols.push(symbol);
                                }
                            }
                        }
                    }
                    _ => {}
                };
                (index_file, stack)
            },
        );
        index_file
    }

    fn watch_and_index_changed_files(index: &IndexRef, channel: mpsc::Receiver<IndexerMessage>) {
        log::info!("Watching workspace files");
        for msg in channel {
            match msg {
                IndexerMessage::IndexModifiedFileBuffer(path, tree, content) => {
                    log::info!("Request to index file buffer for {path:?}");
                    let index_file = Self::index_parse_tree(&tree, content.as_bytes(), path);
                    index.get_mut().update_file(index_file);
                }
                IndexerMessage::StopIndexing => {
                    break;
                }
            }
        }
    }

    fn should_index_file(path: &PathBuf) -> bool {
        match path.extension().and_then(OsStr::to_str) {
            Some("pkg" | "vw" | "wo" | "sl" | "dd" | "src" | "dg" | "bp" | "rv" | "fd" | "inc") => {
                true
            }
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
    ObjectDefinition,
    MethodProcedureDefinition,
    MethodFunctionDefinition,
    PropertyDefinition,
    StructDeclaration,
    StructMember,
    GlobalVariableDeclaration,
    AliasDefinition,
    MixinClass,
    PopStackSymbol,
}

impl Index {
    fn update_file(&mut self, index_file: IndexFile) {
        let file_ref = IndexFileRef::from(&index_file.path);
        let old_index_file = self.files.insert(file_ref.clone(), index_file);
        let new_index_file = self.files.get(&file_ref);
        let symbols_diff = SymbolsDiff::diff_index_files(old_index_file.as_ref(), new_index_file);
        self.lookup_tables.update_symbols(symbols_diff, &file_ref);
        self.lookup_tables.update_dataflex_table_references(
            old_index_file.as_ref().and_then(|f| f.tables.as_deref()),
            new_index_file.and_then(|f| f.tables.as_deref()),
            &file_ref,
        );
    }
}

impl IndexFile {
    pub fn with_parse_tree(tree: &tree_sitter::Tree, content: &[u8]) -> Self {
        Indexer::index_parse_tree(tree, content, PathBuf::new())
    }
}

impl DataFlexDataType {
    fn with_typedecl_node(node: tree_sitter::Node, content: &[u8]) -> Option<Self> {
        let type_name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(content).ok())
            .unwrap_or_default();
        let array_dimension_count = node
            .children_by_field_name("array", &mut node.walk())
            .count();
        if array_dimension_count == 0 {
            Some(Self::Simple(type_name.into()))
        } else {
            Some(Self::Array(type_name.into(), array_dimension_count))
        }
    }
}

impl MetadataTagSet {
    fn associated_metadata_tag_sets(
        symbol_node: &tree_sitter::Node,
        content: &[u8],
    ) -> Option<Vec<MetadataTagSet>> {
        let tag_group = symbol_node
            .prev_sibling()
            .filter(|n| n.kind() == "metadata_tag_group")?;
        let tag_sets = tag_group
            .children_by_field_name("tag_set", &mut tag_group.walk())
            .map(|tag_set| {
                let tags = tag_set
                    .children_by_field_name("tag", &mut tag_set.walk())
                    .filter_map(|tag| {
                        let name = tag.child_by_field_name("name").and_then(|name| {
                            Some(SymbolName::from(name.utf8_text(content).ok()?))
                        })?;
                        let value = tag
                            .child_by_field_name("value")
                            .and_then(|value| Some(String::from(value.utf8_text(content).ok()?)))?;
                        Some(MetadataTag { name, value })
                    })
                    .collect();
                MetadataTagSet { tags }
            })
            .collect();
        Some(tag_sets)
    }
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
        Indexer::index_test_content("Use cWebView.pkg\n", "test.vw".into(), &index_ref);

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
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 0, column: 6 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 1, column: 9 } }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_composite() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Composite cMyClass is a cBaseClass\nEnd_Composite\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 0, column: 10 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 1, column: 13 } }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_class_procedure_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello\n    End_Procedure\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 0, column: 6 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 3, column: 9 } }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 1, column: 14 }, range: SourceRange { start: SourceLocation { line: 1, column: 4 }, end: SourceLocation { line: 2, column: 17 } }, symbol_path: SymbolPath(\"cMyClass.SayHello\"), kind: Msg, parameters: [], return_type: None, metadata: [] })], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_class_procedure_set_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure Set Server String sServer\n    End_Procedure\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 0, column: 6 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 3, column: 9 } }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 1, column: 18 }, range: SourceRange { start: SourceLocation { line: 1, column: 4 }, end: SourceLocation { line: 2, column: 17 } }, symbol_path: SymbolPath(\"cMyClass.Server\"), kind: Set, parameters: [(SymbolName(\"sServer\"), DataFlexDataType(\"String\"))], return_type: None, metadata: [] })], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_class_function_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Function SayHello Returns String\n    End_Function\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 0, column: 6 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 3, column: 9 } }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 1, column: 13 }, range: SourceRange { start: SourceLocation { line: 1, column: 4 }, end: SourceLocation { line: 2, column: 16 } }, symbol_path: SymbolPath(\"cMyClass.SayHello\"), kind: Get, parameters: [], return_type: Some(DataFlexDataType(\"String\")), metadata: [] })], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_class_method_with_parameters() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello String sName\n    End_Procedure\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 0, column: 6 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 3, column: 9 } }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 1, column: 14 }, range: SourceRange { start: SourceLocation { line: 1, column: 4 }, end: SourceLocation { line: 2, column: 17 } }, symbol_path: SymbolPath(\"cMyClass.SayHello\"), kind: Msg, parameters: [(SymbolName(\"sName\"), DataFlexDataType(\"String\"))], return_type: None, metadata: [] })], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_class_property() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure Construct_Object\n        Property Integer piTest 0\n    End_Procedure\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 0, column: 6 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 4, column: 9 } }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 1, column: 14 }, range: SourceRange { start: SourceLocation { line: 1, column: 4 }, end: SourceLocation { line: 3, column: 17 } }, symbol_path: SymbolPath(\"cMyClass.Construct_Object\"), kind: Msg, parameters: [], return_type: None, metadata: [] }), Property(VariableSymbol { location: SourceLocation { line: 2, column: 25 }, range: SourceRange { start: SourceLocation { line: 2, column: 8 }, end: SourceLocation { line: 3, column: 0 } }, symbol_path: SymbolPath(\"cMyClass.piTest\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_object() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Object oMyObj is a cBaseClass\nEnd_Object\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Object(ClassSymbol { location: SourceLocation { line: 0, column: 7 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 1, column: 10 } }, symbol_path: SymbolPath(\"oMyObj\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_nested_object() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Object oMyObj is a cBaseClass\n    Object oMyInner is a cBaseClass\n    End_Object\nEnd_Object\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Object(ClassSymbol { location: SourceLocation { line: 0, column: 7 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 3, column: 10 } }, symbol_path: SymbolPath(\"oMyObj\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [Object(ClassSymbol { location: SourceLocation { line: 1, column: 11 }, range: SourceRange { start: SourceLocation { line: 1, column: 4 }, end: SourceLocation { line: 2, column: 14 } }, symbol_path: SymbolPath(\"oMyObj.oMyInner\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [], metadata: [] })], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_object_method() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Object oMyObj is a cBaseClass\n    Procedure SayHello\n    End_Procedure\nEnd_Object\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Object(ClassSymbol { location: SourceLocation { line: 0, column: 7 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 3, column: 10 } }, symbol_path: SymbolPath(\"oMyObj\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 1, column: 14 }, range: SourceRange { start: SourceLocation { line: 1, column: 4 }, end: SourceLocation { line: 2, column: 17 } }, symbol_path: SymbolPath(\"oMyObj.SayHello\"), kind: Msg, parameters: [], return_type: None, metadata: [] })], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_global_variable() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Global_Variable Integer giMyGlobalVar\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Variable(VariableSymbol { location: SourceLocation { line: 0, column: 24 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 1, column: 0 } }, symbol_path: SymbolPath(\"giMyGlobalVar\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })]"
        );
    }

    #[test]
    fn test_index_struct() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            r#"
Struct tMyStruct
    String sName
    Integer[] iValues
End_Struct
            "#,
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Struct(StructSymbol { location: SourceLocation { line: 1, column: 7 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 5, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct\"), members: [Variable(VariableSymbol { location: SourceLocation { line: 2, column: 11 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 5, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct.sName\"), data_type: DataFlexDataType(\"String\"), metadata: [] }), Variable(VariableSymbol { location: SourceLocation { line: 3, column: 14 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 5, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct.iValues\"), data_type: DataFlexDataType(\"Integer[]\"), metadata: [] })] })]"
        );
    }

    #[test]
    fn test_index_alias() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            r#"
Define MyAlias for MyOriginalSymbol
Define someValue for 1
Define someUndefinedValue
#REPLACE MyNewSymbol MyOldSymbol
            "#,
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Alias(AliasSymbol { location: SourceLocation { line: 1, column: 7 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 2, column: 0 } }, symbol_path: SymbolPath(\"MyAlias\"), alias: Symbol(SymbolName(\"MyOriginalSymbol\")) }), Alias(AliasSymbol { location: SourceLocation { line: 2, column: 7 }, range: SourceRange { start: SourceLocation { line: 2, column: 0 }, end: SourceLocation { line: 3, column: 0 } }, symbol_path: SymbolPath(\"someValue\"), alias: Value(\"1\") }), Alias(AliasSymbol { location: SourceLocation { line: 3, column: 7 }, range: SourceRange { start: SourceLocation { line: 3, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"someUndefinedValue\"), alias: Value(\"\") }), Alias(AliasSymbol { location: SourceLocation { line: 4, column: 9 }, range: SourceRange { start: SourceLocation { line: 4, column: 0 }, end: SourceLocation { line: 5, column: 0 } }, symbol_path: SymbolPath(\"MyNewSymbol\"), alias: Symbol(SymbolName(\"MyOldSymbol\")) })]"
        );
    }

    #[test]
    fn test_index_mixin_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            r#"
Class cFoo is a cBar
    Import_Class_Protocol cMyMixin
    Import_Class_Protocol cMyOtherMixin
End_Class
            "#,
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 1, column: 6 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 9 } }, symbol_path: SymbolPath(\"cFoo\"), superclass: SymbolName(\"cBar\"), mixins: [SymbolName(\"cMyMixin\"), SymbolName(\"cMyOtherMixin\")], members: [], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_dataflex_table() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            r#"
#REPLACE OrderHeader.Recnum |FN30,0
#REPLACE OrderHeader.Order_Number |FN30,1
#REPLACE OrderHeader.Customer_Number |FN30,2
#REPLACE OrderHeader.Order_Date |FD30,3
            "#,
            "test.fd".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.fd")].tables
            ),
            "Some([DataFlexTable { name: SymbolName(\"OrderHeader\"), columns: [SymbolName(\"File_Number\"), SymbolName(\"Recnum\"), SymbolName(\"Order_Number\"), SymbolName(\"Customer_Number\"), SymbolName(\"Order_Date\")] }])"
        );
    }

    #[test]
    fn test_index_class_metadata() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            r#"
{ Visibility = Private }
Class cFoo is a cBar
End_Class
            "#,
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 2, column: 6 }, range: SourceRange { start: SourceLocation { line: 2, column: 0 }, end: SourceLocation { line: 3, column: 9 } }, symbol_path: SymbolPath(\"cFoo\"), superclass: SymbolName(\"cBar\"), mixins: [], members: [], metadata: [MetadataTagSet { tags: [MetadataTag { name: SymbolName(\"Visibility\"), value: \"Private\" }] }] })]"
        );
    }

    #[test]
    fn test_index_method_metadata() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            r#"
Class cFoo is a cBar
    { Visibility = Private }
    Procedure MyPrivateMethod
    End_Procedure
End_Class
            "#,
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 1, column: 6 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 5, column: 9 } }, symbol_path: SymbolPath(\"cFoo\"), superclass: SymbolName(\"cBar\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 3, column: 14 }, range: SourceRange { start: SourceLocation { line: 3, column: 4 }, end: SourceLocation { line: 4, column: 17 } }, symbol_path: SymbolPath(\"cFoo.MyPrivateMethod\"), kind: Msg, parameters: [], return_type: None, metadata: [MetadataTagSet { tags: [MetadataTag { name: SymbolName(\"Visibility\"), value: \"Private\" }] }] })], metadata: [] })]"
        );
    }

    #[test]
    fn test_index_property_metadata() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            r#"
Class cFoo is a cBar
    Procedure Construct_Object
        { Visibility = Private }
        Property Integer piMyProperty
    End_Procedure
End_Class
            "#,
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().files[&IndexFileRef::from("test.pkg")].symbols
            ),
            "[Class(ClassSymbol { location: SourceLocation { line: 1, column: 6 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 6, column: 9 } }, symbol_path: SymbolPath(\"cFoo\"), superclass: SymbolName(\"cBar\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 2, column: 14 }, range: SourceRange { start: SourceLocation { line: 2, column: 4 }, end: SourceLocation { line: 5, column: 17 } }, symbol_path: SymbolPath(\"cFoo.Construct_Object\"), kind: Msg, parameters: [], return_type: None, metadata: [] }), Property(VariableSymbol { location: SourceLocation { line: 4, column: 25 }, range: SourceRange { start: SourceLocation { line: 4, column: 8 }, end: SourceLocation { line: 5, column: 0 } }, symbol_path: SymbolPath(\"cFoo.piMyProperty\"), data_type: DataFlexDataType(\"Integer\"), metadata: [MetadataTagSet { tags: [MetadataTag { name: SymbolName(\"Visibility\"), value: \"Private\" }] }] })], metadata: [] })]"
        );
    }
}
