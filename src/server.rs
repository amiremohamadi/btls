use super::{
    analyzer::semantic_analyzer::SemanticAnalyzer, btf::Btf, client::Client, storage::Storage,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::{
    LanguageServer, LspService, Server,
    jsonrpc::Result,
    lsp_types::{
        CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
        DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, InitializeParams,
        InitializeResult, InitializedParams, Location, MessageType, OneOf, ReferenceParams,
        ServerCapabilities,
    },
};

struct Backend {
    context: Context,
}

pub struct Context {
    pub client: Client,
    pub storage: Arc<Mutex<Storage>>,
    pub analyzer: Mutex<SemanticAnalyzer>,
    pub btf: Arc<Btf>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), ">".to_string()]),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
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
        let Ok(path) = params
            .text_document_position
            .text_document
            .uri
            .to_file_path()
        else {
            return Ok(None);
        };
        let pos = params.text_document_position.position;
        super::completion_provider::completion(&self.context, &path, pos).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let Ok(path) = params
            .text_document_position_params
            .text_document
            .uri
            .to_file_path()
        else {
            return Ok(None);
        };
        let pos = params.text_document_position_params.position;
        super::navigation_provider::goto_definition(&self.context, &path, pos).await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let Ok(path) = params
            .text_document_position
            .text_document
            .uri
            .to_file_path()
        else {
            return Ok(None);
        };
        super::navigation_provider::references(&self.context, &path, params).await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let Ok(path) = params.text_document.uri.to_file_path() else {
            return;
        };
        self.context.storage.lock().await.load(
            &path,
            &params.text_document.text,
            params.text_document.version,
        );

        super::diagnostic_provider::publish_diagnostics(&self.context, params.text_document.uri)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Ok(path) = params.text_document.uri.to_file_path() else {
            return;
        };
        let Some(changes) = params.content_changes.first() else {
            return;
        };
        self.context
            .storage
            .lock()
            .await
            .load(&path, &changes.text, params.text_document.version);

        super::diagnostic_provider::publish_diagnostics(&self.context, params.text_document.uri)
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

pub async fn run() {
    let analyzer = SemanticAnalyzer::new();
    let (service, socket) = LspService::new(move |client| {
        let client = Client::new(client);
        let storage = Arc::new(Mutex::new(Storage::new()));
        let context = Context {
            client,
            storage,
            analyzer: tokio::sync::Mutex::new(analyzer),
            btf: Arc::new(Btf::new()),
        };
        Backend { context }
    });

    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
