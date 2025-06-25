use std::path::PathBuf;
use std::sync::{Arc, OnceLock, Weak};

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::dataflex_document::DataFlexDocument;
use crate::index;

pub struct DataFlexLanguageServer {
    inner: Arc<DataFlexLanguageServerInner>,
}

struct DataFlexLanguageServerInner {
    client: Client,
    open_files: DashMap<Url, DataFlexDocument>,
    workspace_root: OnceLock<PathBuf>,
    indexer: OnceLock<index::Indexer>,
}

struct IndexerObserver {
    inner: Weak<DataFlexLanguageServerInner>,
    runtime: tokio::runtime::Handle,
}

impl index::IndexerObserver for IndexerObserver {
    fn state_transition(&self, old_state: index::IndexerState, new_state: index::IndexerState) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };

        log::info!(
            "Indexing state changed from {:?} to {:?}",
            old_state,
            new_state
        );
        match (old_state, new_state) {
            (index::IndexerState::InitialIndexing, index::IndexerState::Inactive) => {
                for mut file in inner.open_files.iter_mut() {
                    file.update_syntax_map();
                }

                self.runtime.spawn(async move {
                    _ = inner.client.semantic_tokens_refresh().await;
                });
            }
            _ => (),
        }
    }
}

impl DataFlexLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            inner: Arc::new(DataFlexLanguageServerInner {
                client,
                open_files: DashMap::new(),
                workspace_root: OnceLock::new(),
                indexer: OnceLock::new(),
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

        let semantic_tokens_options = if let Some(_) = params
            .capabilities
            .text_document
            .and_then(|t| t.semantic_tokens)
        {
            Some(SemanticTokensServerCapabilities::from(
                SemanticTokensOptions {
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    legend: SemanticTokensLegend {
                        token_types: vec![SemanticTokenType::KEYWORD, SemanticTokenType::CLASS],
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
                completion_provider: Some(Default::default()),
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
            .map(|ref path| index::WorkspaceInfo::load_from_path(path))
            .unwrap_or(index::WorkspaceInfo::new());

        _ = self.inner.indexer.set(index::Indexer::new(
            workspace_info,
            index::IndexerConfig::new(),
        ));
        self.inner
            .indexer
            .get()
            .unwrap()
            .start_indexing(IndexerObserver {
                inner: Arc::downgrade(&self.inner),
                runtime: tokio::runtime::Handle::current(),
            });

        self.inner
            .client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        log::info!("Start tracking {}", params.text_document.uri);
        self.inner.open_files.insert(
            params.text_document.uri,
            DataFlexDocument::new(
                &params.text_document.text,
                self.inner.indexer.get().unwrap().get_index().clone(),
            ),
        );
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.inner.open_files.remove(&params.text_document.uri);
        log::info!("Stop tracking {}", params.text_document.uri);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        log::trace!(
            "Got a textDocument/didChange notification for {}",
            params.text_document.uri.as_str()
        );

        self.inner
            .open_files
            .get_mut(&params.text_document.uri)
            .unwrap()
            .edit_content(&params.content_changes);
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
        let location = self
            .inner
            .open_files
            .get(&params.text_document_position_params.text_document.uri)
            .unwrap()
            .find_definition(params.text_document_position_params.position);
        if let Some(location) = location {
            Ok(Some(GotoDefinitionResponse::Scalar(location)))
        } else {
            Ok(None)
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        log::info!("completion request");
        let completions = self
            .inner
            .open_files
            .get(&params.text_document_position.text_document.uri)
            .unwrap()
            .code_completion(params.text_document_position.position);
        if let Some(completions) = completions {
            Ok(Some(CompletionResponse::List(CompletionList {
                is_incomplete: false,
                items: completions,
            })))
        } else {
            Ok(None)
        }
    }
}
