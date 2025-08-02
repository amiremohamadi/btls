use tower_lsp::{
    Client, LanguageServer, LspService, Server,
    jsonrpc::Result,
    lsp_types::{
        CompletionItem, CompletionOptions, CompletionParams, CompletionResponse, InitializeParams,
        InitializeResult, InitializedParams, MessageType, ServerCapabilities,
    },
};

struct Backend {
    context: Context,
}

struct Context {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                completion_provider: Some(CompletionOptions::default()),
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

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        let items = vec![CompletionItem::new_simple(
            "BEGIN".to_string(),
            "BEGIN probe".to_string(),
        )];
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

pub async fn run() {
    let (service, socket) = LspService::new(move |client| {
        let context = Context { client };
        Backend { context }
    });

    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
