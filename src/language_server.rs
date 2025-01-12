use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

#[derive(Debug)]
pub struct DataFlexLanguageServer {
    client: Client,
}

impl DataFlexLanguageServer {
    pub fn new(client: Client) -> Self {
        Self { client }
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
}
