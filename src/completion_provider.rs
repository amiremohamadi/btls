use super::builtins::BUILTINS;
use super::server::Context;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
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
    context: &Context,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let mut analyzer = context.analyzer.lock().await;
    let variables = match analyzer
        .analyze(params.text_document_position.text_document.uri.path())
        .map_err(|_| Error::new(ErrorCode::InternalError))
    {
        Ok(f) => f
            .variables
            .iter()
            .map(|x| CompletionItem {
                label: x.to_string(),
                kind: Some(CompletionItemKind::VARIABLE),
                ..Default::default()
            })
            .collect(),
        _ => Vec::new(),
    };

    let builtin_keywords =
        builtin_to_completion_item!(BUILTINS.keywords, CompletionItemKind::KEYWORD);
    let builtin_funcs =
        builtin_to_completion_item!(BUILTINS.functions, CompletionItemKind::FUNCTION);

    Ok(Some(CompletionResponse::Array(
        variables
            .into_iter()
            .chain(builtin_keywords)
            .chain(builtin_funcs)
            .collect(),
    )))
}
