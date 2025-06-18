use std::path::PathBuf;
use std::sync::OnceLock;

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::dataflex_document::DataFlexDocument;
use crate::index;

pub struct DataFlexLanguageServer {
    client: Client,
    open_files: DashMap<Url, DataFlexDocument>,
    workspace_root: OnceLock<PathBuf>,
    indexer: OnceLock<index::Indexer>,
}

impl DataFlexLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            open_files: DashMap::new(),
            workspace_root: OnceLock::new(),
            indexer: OnceLock::new(),
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

        _ = self.workspace_root.set(workspace_root.unwrap_or_default());

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
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        log::info!("initialized() called");

        let workspace_info = self
            .workspace_root
            .get()
            .map(|ref path| index::WorkspaceInfo::load_from_path(path))
            .unwrap_or(index::WorkspaceInfo::new());

        _ = self.indexer.set(index::Indexer::new(
            workspace_info,
            index::IndexerConfig::new(),
        ));
        self.indexer.get().unwrap().start_indexing();

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        log::info!("Start tracking {}", params.text_document.uri);
        self.open_files.insert(
            params.text_document.uri,
            DataFlexDocument::new(
                &params.text_document.text,
                self.indexer.get().unwrap().get_index().clone(),
            ),
        );
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.open_files.remove(&params.text_document.uri);
        log::info!("Stop tracking {}", params.text_document.uri);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        log::trace!(
            "Got a textDocument/didChange notification for {}",
            params.text_document.uri.as_str()
        );

        self.open_files
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
}
