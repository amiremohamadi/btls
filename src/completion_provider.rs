use super::builtins::BUILTINS;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, Documentation,
    MarkupContent, MarkupKind,
};

macro_rules! builtin_to_completion_item {
    ($collection:expr, $kind:expr) => {
        $collection.iter().map(|x| CompletionItem {
            label: x.name.to_string(),
            kind: Some($kind),
            detail: Some(x.detail.to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: x.documentation.to_string(),
            })),
            ..Default::default()
        })
    };
}

pub async fn completion(
    context: &super::server::Context,
    _: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let analyzer = context.analyzer.lock().await;
    let variables: Vec<_> = analyzer
        .variables
        .iter()
        .map(|x| CompletionItem::new_simple(x.to_string(), "".to_string()))
        .collect();

    let builtin_keywords =
        builtin_to_completion_item!(BUILTINS.keywords, CompletionItemKind::KEYWORD);
    let builtin_funcs =
        builtin_to_completion_item!(BUILTINS.functions, CompletionItemKind::FUNCTION);

    Ok(Some(CompletionResponse::Array(
        builtin_keywords
            .chain(builtin_funcs)
            .chain(variables)
            .collect(),
    )))
}
