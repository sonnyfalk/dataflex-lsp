mod dataflex_document;
mod language_server;
mod logging;

#[tokio::main]
async fn main() {
    logging::initialize_logging();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) =
        tower_lsp::LspService::new(|client| language_server::DataFlexLanguageServer::new(client));
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
