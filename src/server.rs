use super::client::Client;
use super::parser::semantic_analyzer::SemanticAnalyzer;
use tokio::sync::Mutex;
use tower_lsp::{
    LanguageServer, LspService, Server,
    jsonrpc::Result,
    lsp_types::{
        CompletionOptions, CompletionParams, CompletionResponse, DidOpenTextDocumentParams,
        InitializeParams, InitializeResult, InitializedParams, MessageType, ServerCapabilities,
    },
};

struct Backend {
    context: Context,
}

pub struct Context {
    pub client: Client,
    pub analyzer: Mutex<SemanticAnalyzer>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                completion_provider: Some(CompletionOptions::default()),
                text_document_sync: Some(tower_lsp::lsp_types::TextDocumentSyncCapability::Kind(
                    tower_lsp::lsp_types::TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.context
            .client
            .log_message(
                MessageType::INFO,
                "btls (bpftrace language server) initialized",
            )
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        super::completion_provider::completion(&self.context, params).await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        // let mut analyzer = self.context.analyzer.lock().await;
        super::diagnostic_provider::publish_diagnostics(&self.context, params.text_document.uri)
            .await;
        // analyzer.analyze(params.text_document.uri.path());
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

pub async fn run() {
    let analyzer = SemanticAnalyzer::new();
    let (service, socket) = LspService::new(move |client| {
        let client = Client::new(client);
        let context = Context {
            client,
            analyzer: tokio::sync::Mutex::new(analyzer),
        };
        Backend { context }
    });

    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
