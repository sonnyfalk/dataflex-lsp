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
    progress_reporter: Arc<IndexerProgressReporter>,
    tasks: Mutex<tokio::task::JoinSet<()>>,
}

struct IndexerProgressReporter {
    inner: Weak<DataFlexLanguageServerInner>,
    completed_notification: Arc<tokio::sync::Notify>,
    task: tokio::sync::Mutex<tokio::task::JoinSet<()>>,
}

impl DataFlexLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            inner: Arc::new(DataFlexLanguageServerInner {
                client,
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

        let workspace_info = self
            .inner
            .workspace_root
            .get()
            .map(|path| index::WorkspaceInfo::load_from_path(path))
            .unwrap_or(index::WorkspaceInfo::new());

        _ = self.inner.indexer.set(index::Indexer::new(
            workspace_info,
            index::IndexerConfig::new(),
        ));
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
                progress_reporter: Arc::new(IndexerProgressReporter::new(Arc::downgrade(
                    &self.inner,
                ))),
                tasks: Mutex::new(tokio::task::JoinSet::new()),
            });

        _ = self
            .inner
            .client
            .register_capability(vec![Registration {
                id: String::from("dataflex-lsp"),
                method: String::from("workspace/didChangeConfiguration"),
                register_options: None,
            }])
            .await;

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
        log::info!("Start tracking {}", params.text_document.uri);
        let file_path = params.text_document.uri.to_file_path().unwrap_or_default();
        self.inner.open_files.insert(
            params.text_document.uri,
            OpenFile::new(DataFlexDocument::new(
                file_path,
                &params.text_document.text,
                self.inner.indexer.get().unwrap().get_index().clone(),
            )),
        );
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.inner.open_files.remove(&params.text_document.uri);
        log::info!("Stop tracking {}", params.text_document.uri);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        log::info!(
            "Got a textDocument/didChange notification for {}",
            params.text_document.uri.as_str()
        );

        let followup_edit =
            if let Some(mut open_file) = self.inner.open_files.get_mut(&params.text_document.uri) {
                let followup_edits = open_file.doc.edit_content(&params.content_changes);
                open_file.modified = true;
                self.inner.edited_files_notification.notify_one();

                followup_edits.map(|edits| TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier::new(
                        params.text_document.uri,
                        params.text_document.version,
                    ),
                    edits: edits.into_iter().map(OneOf::Left).collect(),
                })
            } else {
                None
            };

        if let Some(followup_edit) = followup_edit {
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
            .open_files
            .get(&params.text_document.uri)
            .unwrap()
            .doc
            .semantic_tokens_full()
            .unwrap();

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
            .open_files
            .get(&params.text_document_position_params.text_document.uri)
            .unwrap()
            .doc
            .find_definition(params.text_document_position_params.position);
        if let Some(locations) = locations {
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        } else {
            Ok(None)
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let completions = self
            .inner
            .open_files
            .get(&params.text_document_position.text_document.uri)
            .unwrap()
            .doc
            .code_completion(
                params.text_document_position.position,
                params
                    .context
                    .is_some_and(|c| c.trigger_kind == CompletionTriggerKind::TRIGGER_CHARACTER),
            );
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
            .open_files
            .get(&params.text_document_position_params.text_document.uri)
            .unwrap()
            .doc
            .symbol_declaration(params.text_document_position_params.position);
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
            .open_files
            .get(&params.text_document_position_params.text_document.uri)
            .unwrap()
            .doc
            .signature_help(params.text_document_position_params.position);
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
            .open_files
            .get(&params.text_document_position_params.text_document.uri)
            .unwrap()
            .doc
            .document_highlight(params.text_document_position_params.position);

        Ok(highlights)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let symbols = self
            .inner
            .open_files
            .get(&params.text_document.uri)
            .unwrap()
            .doc
            .document_symbols();

        Ok(Some(symbols.into()))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let code_lens_items = self
            .inner
            .open_files
            .get(&params.text_document.uri)
            .unwrap()
            .doc
            .code_lens_items();
        Ok(Some(code_lens_items))
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
        log::info!("config changed");
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

            inner
                .open_files
                .iter_mut()
                .filter(|open_file| open_file.modified)
                .for_each(|mut open_file| {
                    if let Some(tree) = open_file.doc.tree().cloned()
                        && let Some(file_path) = open_file.key().to_file_path().ok()
                        && let Some(indexer) = inner.indexer.get()
                    {
                        let content = open_file.doc.text_content();
                        indexer.index_modified_file_buffer(file_path, tree, content);
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
        let progress_reporter = self.progress_reporter.clone();

        log::info!(
            "Indexing state changed from {:?} to {:?}",
            old_state,
            new_state
        );
        match (old_state, new_state) {
            (index::IndexerState::Initializing, index::IndexerState::InitialIndexing) => {
                self.tasks.lock().unwrap().spawn_on(
                    async move {
                        progress_reporter.start_progress_report().await;
                    },
                    &self.runtime,
                );
            }
            (index::IndexerState::InitialIndexing, index::IndexerState::Inactive) => {
                for mut file in inner.open_files.iter_mut() {
                    file.doc.update_syntax_map();
                }

                self.tasks.lock().unwrap().spawn_on(
                    async move {
                        _ = inner.client.semantic_tokens_refresh().await;
                        _ = inner.client.code_lens_refresh().await;
                        progress_reporter.end_progress_report().await;
                        drop(progress_reporter);

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
    fn new(inner: Weak<DataFlexLanguageServerInner>) -> Self {
        Self {
            inner,
            completed_notification: Arc::new(tokio::sync::Notify::new()),
            task: tokio::sync::Mutex::new(tokio::task::JoinSet::new()),
        }
    }

    fn indexing_progress_token() -> NumberOrString {
        NumberOrString::String("Indexing".into())
    }

    async fn start_progress_report(&self) {
        if let Some(inner) = self.inner.upgrade() {
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
        } else {
            return;
        }

        let inner = self.inner.clone();
        let completed_notification = self.completed_notification.clone();
        self.task.lock().await.spawn(async move {
            let start_file_count = inner
                .upgrade()
                .and_then(|inner| {
                    inner
                        .indexer
                        .get()
                        .map(|indexer| indexer.indexed_file_count())
                })
                .unwrap_or_default();
            while tokio::time::timeout(
                std::time::Duration::from_millis(250),
                completed_notification.notified(),
            )
            .await
            .is_err()
            {
                if let Some(inner) = inner.upgrade()
                    && let Some(file_count) = inner
                        .indexer
                        .get()
                        .map(|indexer| indexer.indexed_file_count() - start_file_count)
                {
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
                } else {
                    break;
                }
            }
        });
    }

    async fn end_progress_report(&self) {
        self.completed_notification.notify_one();
        self.task.lock().await.join_next().await;

        if let Some(inner) = self.inner.upgrade() {
            _ = inner
                .client
                .send_notification::<notification::Progress>(ProgressParams {
                    token: Self::indexing_progress_token(),
                    value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                        WorkDoneProgressEnd {
                            message: Some("Indexing complete".into()),
                        },
                    )),
                })
                .await;
        }
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
