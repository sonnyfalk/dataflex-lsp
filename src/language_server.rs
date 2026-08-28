use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::dataflex_document::DataFlexDocument;
use crate::index;
use crate::settings::Settings;

pub struct DataFlexLanguageServer {
    inner: Arc<DataFlexLanguageServerInner>,
}

struct DataFlexLanguageServerInner {
    client: Client,
    client_supports_apply_edit_preserve_selection: OnceLock<bool>,
    open_files: DashMap<Url, OpenFile>,
    workspace_root: OnceLock<PathBuf>,
    indexer: OnceLock<index::Indexer>,
    edited_files_notification: tokio::sync::Notify,
}

struct OpenFile {
    doc: DataFlexDocument,
    modified: bool,
}

struct IndexerCoordinator {
    inner: Weak<DataFlexLanguageServerInner>,
    runtime: tokio::runtime::Handle,
    progress_reporter: IndexerProgressReporter,
    tasks: Mutex<tokio::task::JoinSet<()>>,
}

struct IndexerProgressReporter {
    _task: tokio::task::JoinSet<()>,
    channel: tokio::sync::watch::Sender<index::IndexerState>,
}

impl DataFlexLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            inner: Arc::new(DataFlexLanguageServerInner {
                client,
                client_supports_apply_edit_preserve_selection: OnceLock::new(),
                open_files: DashMap::new(),
                workspace_root: OnceLock::new(),
                indexer: OnceLock::new(),
                edited_files_notification: tokio::sync::Notify::new(),
            }),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for DataFlexLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let workspace_root = params
            .workspace_folders
            .as_ref()
            .unwrap()
            .first()
            .unwrap()
            .uri
            .to_file_path()
            .ok();
        log::info!(
            "initialize - client: {}, path: {:?}",
            params.client_info.as_ref().unwrap().name,
            workspace_root
        );

        _ = self
            .inner
            .client_supports_apply_edit_preserve_selection
            .set(
                params
                    .capabilities
                    .experimental
                    .and_then(|exp| {
                        exp.as_object()
                            .and_then(|obj| obj.get("dataFlexApplyEditPreserveSelection"))
                            .and_then(|value| value.as_bool())
                    })
                    .unwrap_or(false),
            );

        _ = self
            .inner
            .workspace_root
            .set(workspace_root.unwrap_or_default());

        let semantic_tokens_options = if params
            .capabilities
            .text_document
            .and_then(|t| t.semantic_tokens)
            .is_some()
        {
            Some(SemanticTokensServerCapabilities::from(
                SemanticTokensOptions {
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    legend: SemanticTokensLegend {
                        token_types: vec![
                            SemanticTokenType::KEYWORD,
                            SemanticTokenType::CLASS,
                            SemanticTokenType::METHOD,
                            SemanticTokenType::PROPERTY,
                            SemanticTokenType::INTERFACE,
                            SemanticTokenType::FUNCTION,
                            SemanticTokenType::STRUCT,
                            SemanticTokenType::ENUM_MEMBER,
                            SemanticTokenType::NAMESPACE,
                        ],
                        token_modifiers: vec![],
                    },
                    ..Default::default()
                },
            ))
        } else {
            None
        };
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        ..Default::default()
                    },
                )),
                semantic_tokens_provider: semantic_tokens_options,
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![String::from("."), String::from(" ")]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![String::from(" "), String::from("(")]),
                    ..Default::default()
                }),
                document_highlight_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        log::info!("initialized() called");

        if let Ok(configs) = self
            .inner
            .client
            .configuration(vec![ConfigurationItem {
                section: Some(String::from("dataflex-lsp")),
                ..Default::default()
            }])
            .await
            && let Some(settings) = configs
                .into_iter()
                .next()
                .and_then(|v| serde_json::from_value::<Settings>(v).ok())
        {
            Settings::set(settings);
        }

        let workspace_info = self
            .inner
            .workspace_root
            .get()
            .map(|path| index::WorkspaceInfo::load_from_path(path))
            .unwrap_or(index::WorkspaceInfo::new());

        _ = self.inner.indexer.set(index::Indexer::new(workspace_info));
        if self
            .inner
            .indexer
            .get()
            .is_some_and(|indexer| indexer.load_index())
        {
            log::info!("Loaded index");
        }

        self.inner
            .indexer
            .get()
            .unwrap()
            .start_indexing(IndexerCoordinator {
                inner: Arc::downgrade(&self.inner),
                runtime: tokio::runtime::Handle::current(),
                progress_reporter: IndexerProgressReporter::new(Arc::downgrade(&self.inner)).await,
                tasks: Mutex::new(tokio::task::JoinSet::new()),
            });

        _ = self
            .inner
            .client
            .register_capability(vec![
                Registration {
                    id: String::from("dataflex-lsp/workspace/didChangeConfiguration"),
                    method: String::from("workspace/didChangeConfiguration"),
                    register_options: None,
                },
                Registration {
                    id: String::from("dataflex-lsp/workspace/didChangeWatchedFiles"),
                    method: String::from("workspace/didChangeWatchedFiles"),
                    register_options: Some(
                        serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                            watchers: vec![FileSystemWatcher {
                                glob_pattern: GlobPattern::String("**/*".into()),
                                kind: None,
                            }],
                        })
                        .unwrap(),
                    ),
                },
            ])
            .await;

        self.inner
            .client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        log::info!("shutdown() called");
        if let Some(indexer) = self.inner.indexer.get() {
            indexer.stop_indexing();
            indexer.save_index();
        };
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        log::trace!("Start tracking {}", params.text_document.uri);
        self.inner
            .open_file(params.text_document.uri, &params.text_document.text);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.inner.close_file(&params.text_document.uri);
        log::trace!("Stop tracking {}", params.text_document.uri);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        log::trace!(
            "Got a textDocument/didChange notification for {}",
            params.text_document.uri.as_str()
        );

        let (followup_edit, preserve_selection) = if let Some((index, mut open_file)) = self
            .inner
            .get_index_and_open_file_mut(&params.text_document.uri)
        {
            let followup_edits = open_file.doc.edit_content(&params.content_changes, &index);
            open_file.modified = true;
            self.inner.edited_files_notification.notify_one();

            let mut preserve_selection = false;
            (
                followup_edits.map(|mut edits| {
                    if edits
                        .first()
                        .is_some_and(|e| e.range.start.line >= open_file.doc.line_count() as u32)
                    {
                        let end = open_file
                            .doc
                            .lsp_position_from_point(open_file.doc.end_of_document());
                        edits.insert(
                            0,
                            TextEdit {
                                range: Range {
                                    start: end,
                                    end: end,
                                },
                                new_text: String::from("\n"),
                            },
                        );
                        preserve_selection = true;
                    }
                    TextDocumentEdit {
                        text_document: OptionalVersionedTextDocumentIdentifier::new(
                            params.text_document.uri.clone(),
                            params.text_document.version,
                        ),
                        edits: edits.into_iter().map(OneOf::Left).collect(),
                    }
                }),
                preserve_selection,
            )
        } else {
            (None, false)
        };

        if let Some(followup_edit) = followup_edit {
            if preserve_selection
                && self
                    .inner
                    .client_supports_apply_edit_preserve_selection
                    .get()
                    .copied()
                    .unwrap_or(false)
            {
                _ = self
                    .inner
                    .client
                    .send_request::<custom_lsp_requests::ApplyEditPreserveSelection>(followup_edit)
                    .await;
            } else {
                _ = self
                    .inner
                    .client
                    .apply_edit(WorkspaceEdit {
                        document_changes: Some(DocumentChanges::Edits(vec![followup_edit])),
                        ..Default::default()
                    })
                    .await;
            }
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        log::trace!(
            "Got a textDocument/semanticTokensFull notification for {}",
            params.text_document.uri.as_str()
        );

        let tokens = self
            .inner
            .get_index_and_open_file(&params.text_document.uri)
            .map(|(_, open_file)| open_file.doc.semantic_tokens_full().unwrap())
            .unwrap_or_default();

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            data: tokens,
            ..Default::default()
        })))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let locations = self
            .inner
            .get_index_and_open_file(&params.text_document_position_params.text_document.uri)
            .and_then(|(index, open_file)| {
                open_file
                    .doc
                    .find_definition(params.text_document_position_params.position, &index)
            });
        if let Some(locations) = locations {
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        } else {
            Ok(None)
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let completions = self
            .inner
            .get_index_and_open_file(&params.text_document_position.text_document.uri)
            .and_then(|(index, open_file)| {
                open_file.doc.code_completion(
                    params.text_document_position.position,
                    params.context.is_some_and(|c| {
                        c.trigger_kind == CompletionTriggerKind::TRIGGER_CHARACTER
                    }),
                    &index,
                )
            });
        if let Some(completions) = completions {
            Ok(Some(CompletionResponse::List(CompletionList {
                is_incomplete: false,
                items: completions,
            })))
        } else {
            Ok(None)
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let declaration = self
            .inner
            .get_index_and_open_file_mut(&params.text_document_position_params.text_document.uri)
            .and_then(|(index, open_file)| {
                open_file
                    .doc
                    .symbol_declaration(params.text_document_position_params.position, &index)
            });
        if let Some(declaration) = declaration {
            Ok(Some(Hover {
                contents: HoverContents::Scalar(declaration),
                range: None,
            }))
        } else {
            Ok(None)
        }
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let signature_information = self
            .inner
            .get_index_and_open_file(&params.text_document_position_params.text_document.uri)
            .and_then(|(index, open_file)| {
                open_file
                    .doc
                    .signature_help(params.text_document_position_params.position, &index)
            });
        if let Some(signature_information) = signature_information {
            Ok(Some(SignatureHelp {
                signatures: signature_information,
                active_signature: None,
                active_parameter: None,
            }))
        } else {
            Ok(None)
        }
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let highlights = self
            .inner
            .get_index_and_open_file(&params.text_document_position_params.text_document.uri)
            .and_then(|(_, open_file)| {
                open_file
                    .doc
                    .document_highlight(params.text_document_position_params.position)
            });

        Ok(highlights)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let symbols = self
            .inner
            .get_index_and_open_file(&params.text_document.uri)
            .map(|(_, open_file)| open_file.doc.document_symbols())
            .unwrap_or_default();

        Ok(Some(symbols.into()))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let code_lens_items = self
            .inner
            .get_index_and_open_file(&params.text_document.uri)
            .map(|(index, open_file)| open_file.doc.code_lens_items(&index));
        Ok(code_lens_items)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let Some(index) = self
            .inner
            .indexer
            .get()
            .map(|indexer| indexer.get_index().get())
        else {
            return Ok(None);
        };

        let symbols = if params.query.is_empty() {
            index.top_level_class_and_object_symbols()
        } else {
            index.matching_symbols(&params.query)
        };

        #[allow(deprecated)]
        let symbols = symbols
            .map(|s| SymbolInformation {
                name: s.symbol.name().to_string(),
                kind: SymbolKind::from(s.symbol),
                tags: None,
                deprecated: None,
                location: Location::from(&s),
                container_name: s
                    .symbol
                    .symbol_path()
                    .parent_name()
                    .map(index::SymbolName::to_string),
            })
            .collect();
        Ok(Some(symbols))
    }

    async fn did_change_configuration(&self, _params: DidChangeConfigurationParams) {
        log::trace!("config changed");
        if let Ok(configs) = self
            .inner
            .client
            .configuration(vec![ConfigurationItem {
                section: Some(String::from("dataflex-lsp")),
                ..Default::default()
            }])
            .await
            && let Some(settings) = configs
                .into_iter()
                .next()
                .and_then(|v| serde_json::from_value::<Settings>(v).ok())
        {
            Settings::set(settings);
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        log::trace!("did_change_watched_files: {:?}", params);
        let mut changes = params.changes;
        let modified_files: Vec<PathBuf> = changes
            .extract_if(.., |event| {
                matches!(event.typ, FileChangeType::CHANGED | FileChangeType::CREATED)
            })
            .filter_map(|event| event.uri.to_file_path().ok())
            .filter(|path| path.is_dir() || index::Indexer::should_index_file(path))
            .collect();
        let removed_files: Vec<PathBuf> = changes
            .extract_if(.., |event| matches!(event.typ, FileChangeType::DELETED))
            .filter_map(|event| event.uri.to_file_path().ok())
            .collect();
        if let Some(indexer) = self.inner.indexer.get() {
            if !removed_files.is_empty() {
                indexer.remove_indexed_files(removed_files);
            }
            if !modified_files.is_empty() {
                indexer.index_modified_files(dedup_and_prune_nested_paths(modified_files));
            }
        }
    }
}

fn dedup_and_prune_nested_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let paths: Vec<PathBuf> = paths
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let dirs: Vec<PathBuf> = paths.iter().filter(|p| p.is_dir()).cloned().collect();

    paths
        .into_iter()
        .filter(|p| !dirs.iter().any(|d| p != d && p.starts_with(d)))
        .collect()
}

impl DataFlexLanguageServerInner {
    fn open_file(&self, url: Url, text: &str) {
        let Some(index) = self.indexer.get().map(|indexer| indexer.get_index().get()) else {
            return;
        };
        let file_path = url.to_file_path().unwrap_or_default();
        self.open_files.insert(
            url,
            OpenFile::new(DataFlexDocument::new(file_path, text, &index)),
        );
    }

    fn close_file(&self, url: &Url) {
        self.open_files.remove(&url);
    }

    fn get_index_and_open_file(
        &self,
        url: &Url,
    ) -> Option<(
        impl Deref<Target = index::Index>,
        impl Deref<Target = OpenFile>,
    )> {
        self.indexer
            .get()
            .map(|indexer| indexer.get_index().get())
            .and_then(|index| self.open_files.get(url).map(|open_file| (index, open_file)))
    }

    fn get_index_and_open_file_mut(
        &self,
        url: &Url,
    ) -> Option<(
        impl Deref<Target = index::Index>,
        impl DerefMut<Target = OpenFile>,
    )> {
        self.indexer
            .get()
            .map(|indexer| indexer.get_index().get())
            .and_then(|index| {
                self.open_files
                    .get_mut(url)
                    .map(|open_file| (index, open_file))
            })
    }

    fn for_all_open_files_mut<F: FnMut(&index::Index, &mut OpenFile)>(&self, mut f: F) {
        let Some(index) = self.indexer.get().map(|indexer| indexer.get_index().get()) else {
            return;
        };
        for mut open_file in self.open_files.iter_mut() {
            f(&index, &mut open_file);
        }
    }
}

impl OpenFile {
    fn new(doc: DataFlexDocument) -> Self {
        Self {
            doc,
            modified: false,
        }
    }
}

impl IndexerCoordinator {
    async fn watch_and_index_edited_files(inner: Arc<DataFlexLanguageServerInner>) {
        if let Some(indexer) = inner.indexer.get() {
            indexer.save_index()
        }

        loop {
            inner
                .edited_files_notification
                .notified_debounce(std::time::Duration::from_secs(2))
                .await;

            inner.for_all_open_files_mut(|_, open_file| {
                if open_file.modified
                    && let Some(tree) = open_file.doc.tree().cloned()
                    && let Some(indexer) = inner.indexer.get()
                {
                    let content = open_file.doc.text_content();
                    indexer.index_modified_file_buffer(
                        open_file.doc.file_path().clone(),
                        tree,
                        content,
                    );
                    open_file.modified = false;
                }
            });
        }
    }
}

impl index::IndexerObserver for IndexerCoordinator {
    fn state_transition(&self, old_state: index::IndexerState, new_state: index::IndexerState) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };

        log::info!(
            "Indexing state changed from {:?} to {:?}",
            old_state,
            new_state
        );

        self.progress_reporter.update_indexer_state(new_state);

        match (old_state, new_state) {
            (index::IndexerState::InitialIndexing, index::IndexerState::Inactive) => {
                inner.for_all_open_files_mut(|index, open_file| {
                    open_file.doc.update_syntax_map(index)
                });

                self.tasks.lock().unwrap().spawn_on(
                    async move {
                        _ = inner.client.semantic_tokens_refresh().await;
                        _ = inner.client.code_lens_refresh().await;
                        Self::watch_and_index_edited_files(inner).await;
                    },
                    &self.runtime,
                );
            }
            (_, index::IndexerState::Stopped) => {
                self.tasks.lock().unwrap().abort_all();
            }
            _ => (),
        }
    }
}

impl IndexerProgressReporter {
    async fn new(inner: Weak<DataFlexLanguageServerInner>) -> Self {
        let (tx, rx) = tokio::sync::watch::channel(index::IndexerState::Initializing);
        let mut task = tokio::task::JoinSet::new();
        task.spawn(async move {
            Self::run(inner, rx).await;
        });
        Self {
            _task: task,
            channel: tx,
        }
    }

    fn update_indexer_state(&self, state: index::IndexerState) {
        _ = self.channel.send(state);
    }

    async fn run(
        inner: Weak<DataFlexLanguageServerInner>,
        mut channel: tokio::sync::watch::Receiver<index::IndexerState>,
    ) {
        let mut reporting: Option<usize> = None;
        let timeout_duration = std::time::Duration::from_millis(250);
        loop {
            if reporting.is_some() {
                if matches!(
                    tokio::time::timeout(timeout_duration, channel.changed()).await,
                    Ok(Err(_))
                ) {
                    break;
                }
            } else {
                if channel.changed().await.is_err() {
                    break;
                }
            }
            let state = *channel.borrow_and_update();
            match state {
                index::IndexerState::InitialIndexing | index::IndexerState::Indexing
                    if reporting.is_none() =>
                {
                    if let Some(inner) = inner.upgrade() {
                        reporting = Some(
                            inner
                                .indexer
                                .get()
                                .map(|indexer| indexer.indexed_file_count())
                                .unwrap_or_default(),
                        );
                        Self::start_report(&inner).await;
                    } else {
                        break;
                    }
                }
                index::IndexerState::InitialIndexing | index::IndexerState::Indexing
                    if reporting.is_some() =>
                {
                    if let Some(inner) = inner.upgrade()
                        && let Some(file_count) = inner.indexer.get().map(|indexer| {
                            indexer.indexed_file_count() - reporting.as_ref().unwrap()
                        })
                    {
                        Self::report_progress(&inner, file_count).await;
                    } else {
                        break;
                    }
                }
                index::IndexerState::Inactive if reporting.is_some() => {
                    tokio::time::sleep(timeout_duration).await;
                    if channel.has_changed().unwrap_or_default() {
                        continue;
                    }
                    if let Some(inner) = inner.upgrade() {
                        Self::end_report(&inner).await;
                    }
                    reporting = None;
                }
                index::IndexerState::Stopped => {
                    break;
                }
                _ => {}
            }
        }

        if reporting.is_some()
            && let Some(inner) = inner.upgrade()
        {
            Self::end_report(&inner).await;
        }
    }

    async fn start_report(inner: &DataFlexLanguageServerInner) {
        _ = inner
            .client
            .send_request::<request::WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
                token: Self::indexing_progress_token(),
            })
            .await;

        _ = inner
            .client
            .send_notification::<notification::Progress>(ProgressParams {
                token: Self::indexing_progress_token(),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                    WorkDoneProgressBegin {
                        title: "DataFlex-LSP".into(),
                        message: Some("Indexing...".into()),
                        percentage: None,
                        cancellable: Some(false),
                    },
                )),
            })
            .await;
    }

    async fn report_progress(inner: &DataFlexLanguageServerInner, file_count: usize) {
        _ = inner
            .client
            .send_notification::<notification::Progress>(ProgressParams {
                token: Self::indexing_progress_token(),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(
                    WorkDoneProgressReport {
                        message: Some(format!("Indexing {file_count} files...")),
                        ..Default::default()
                    },
                )),
            })
            .await;
    }

    async fn end_report(inner: &DataFlexLanguageServerInner) {
        _ = inner
            .client
            .send_notification::<notification::Progress>(ProgressParams {
                token: Self::indexing_progress_token(),
                value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
                    message: Some("Indexing complete".into()),
                })),
            })
            .await;
    }

    fn indexing_progress_token() -> NumberOrString {
        NumberOrString::String("Indexing".into())
    }
}

trait NotifyDebounce {
    async fn notified_debounce(&self, duration: std::time::Duration);
}

impl NotifyDebounce for tokio::sync::Notify {
    async fn notified_debounce(&self, duration: std::time::Duration) {
        self.notified().await;
        while tokio::time::timeout(duration, self.notified())
            .await
            .is_ok()
        {}
    }
}

mod custom_lsp_requests {
    use tower_lsp::lsp_types::TextDocumentEdit;
    use tower_lsp::lsp_types::request::Request;

    pub enum ApplyEditPreserveSelection {}

    impl Request for ApplyEditPreserveSelection {
        type Params = TextDocumentEdit;
        type Result = bool;
        const METHOD: &'static str = "dataFlex/applyEditPreserveSelection";
    }
}
