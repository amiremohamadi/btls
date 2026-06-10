use super::analyzer::semantic_analyzer;
use super::builtins::BUILTINS;
use super::server::Context;
use std::path::Path;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Documentation, MarkupContent,
    MarkupKind, Position,
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
    path: &Path,
    position: Position,
) -> Result<Option<CompletionResponse>> {
    let mut analyzer = context.analyzer.lock().await;
    let analyzed = analyzer
        .analyze(context, path)
        .await
        .map_err(|_| Error::new(ErrorCode::InternalError))?;

    let Some(offset) = analyzed.document.line_index.offset(position) else {
        return Ok(None);
    };

    let variables = semantic_analyzer::variables_at(&analyzed.ast, offset)
        .into_iter()
        .map(|x| CompletionItem {
            label: x.to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            ..Default::default()
        })
        .collect::<Vec<_>>();

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
