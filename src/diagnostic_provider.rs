use super::parser::Node;
use super::server::Context;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};

fn to_position(content: &str, pos: usize) -> Position {
    let (line, col) = pest::Position::new(content, pos).unwrap().line_col();
    Position::new((line - 1) as _, (col - 1) as _)
}

pub async fn publish_diagnostics(context: &Context, uri: Url) {
    let mut ast = context.analyzer.lock().await;
    let analyzed_file = ast.analyze(&uri.path());

    let content = std::fs::read_to_string(&uri.path()).unwrap();

    let digs = analyzed_file
        .ast
        .as_node()
        .errors()
        .map(|e| Diagnostic {
            range: Range::new(
                to_position(&content, e.span().start()),
                to_position(&content, e.span().end()),
            ),
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
