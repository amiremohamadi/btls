use super::server::Context;
use tower_lsp::jsonrpc::{Error, ErrorCode};
use tower_lsp::lsp_types::Url;

pub async fn publish_diagnostics(context: &Context, uri: Url) {
    let Ok(path) = uri.to_file_path() else {
        return;
    };

    let config = context.client.config().await;
    if !config.diagnostics {
        return;
    }

    let analyzer = context.analyzer.lock().await;
    let analyzed_file = match analyzer
        .analyze(context, &path)
        .await
        .map_err(|_| Error::new(ErrorCode::InternalError))
    {
        Ok(f) => f,
        _ => return,
    };

    context
        .client
        .publish_diagnostics(uri.clone(), analyzed_file.diagnostics().to_vec(), None)
        .await;
}
