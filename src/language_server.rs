use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::dataflex_document::DataFlexDocument;

pub struct DataFlexLanguageServer {
    client: Client,
    open_files: DashMap<Url, DataFlexDocument>,
}

impl DataFlexLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            open_files: DashMap::new(),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for DataFlexLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        log::info!(
            "initialize - client: {}, path: {}",
            params.client_info.as_ref().unwrap().name,
            params
                .workspace_folders
                .as_ref()
                .unwrap()
                .first()
                .unwrap()
                .uri
                .to_string()
        );
        log::info!("InitializeParams: {:?}", params);

        let semantic_tokens_options = if let Some(_) = params
            .capabilities
            .text_document
            .and_then(|t| t.semantic_tokens)
        {
            Some(SemanticTokensServerCapabilities::from(
                SemanticTokensOptions {
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    legend: SemanticTokensLegend {
                        token_types: vec![SemanticTokenType::KEYWORD],
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
                        change: Some(TextDocumentSyncKind::FULL),
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
            DataFlexDocument::new(params.text_document.text),
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

        let mut params = params;
        if let Some(change) = params.content_changes.pop() {
            self.open_files
                .get_mut(&params.text_document.uri)
                .unwrap()
                .replace_content(change.text);
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
