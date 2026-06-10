use super::parser::Node;
use super::server::Context;
use tower_lsp::jsonrpc::{Error, ErrorCode};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

pub async fn publish_diagnostics(context: &Context, uri: Url) {
    let Ok(path) = uri.to_file_path() else {
        return;
    };

    let config = context.client.config().await;
    if !config.diagnostics {
        return;
    }

    let mut analyzer = context.analyzer.lock().await;
    let analyzed_file = match analyzer
        .analyze(context, &path)
        .await
        .map_err(|_| Error::new(ErrorCode::InternalError))
    {
        Ok(f) => f,
        _ => return,
    };

    let digs = analyzed_file
        .ast
        .as_node()
        .errors()
        .map(|e| Diagnostic {
            range: analyzed_file.document.line_index.range(e.span()),
            severity: Some(DiagnosticSeverity::ERROR),
            message: e.diagnosis(),
            ..Default::default()
        })
        .collect();

    context
        .client
        .publish_diagnostics(uri.clone(), digs, None)
        .await;
}
