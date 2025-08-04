use super::builtins::BUILTINS;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, Documentation,
    MarkupContent, MarkupKind,
};

pub async fn completion(
    context: &super::server::Context,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let analyzer = context.analyzer.lock().await;
    let variables: Vec<_> = analyzer
        .variables
        .iter()
        .map(|x| CompletionItem::new_simple(x.to_string(), "".to_string()))
        .collect();

    let builtin_keywords = BUILTINS.keywords.iter().map(|x| CompletionItem {
        label: x.name.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some(x.detail.to_string()),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: x.documentation.to_string(),
        })),
        ..Default::default()
    });

    Ok(Some(CompletionResponse::Array(
        builtin_keywords.chain(variables).collect(),
    )))
}
