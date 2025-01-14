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
            params.client_info.unwrap().name,
            params
                .workspace_folders
                .unwrap()
                .first()
                .unwrap()
                .uri
                .to_string()
        );

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        ..Default::default()
                    },
                )),
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
}
