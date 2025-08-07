use super::config::Config;
use std::fmt::Display;
use tower_lsp::{
    Client as LSPClient,
    lsp_types::{ConfigurationItem, Diagnostic, MessageType, Url},
};

static BTLS_SECTION: &str = "btls";

pub struct Client {
    pub inner: LSPClient,
}

impl Client {
    pub fn new(client: LSPClient) -> Self {
        Self { inner: client }
    }

    pub async fn log_message<M: Display>(&self, typ: MessageType, message: M) {
        self.inner.log_message(typ, message).await;
    }

    pub async fn publish_diagnostics(
        &self,
        uri: Url,
        diags: Vec<Diagnostic>,
        version: Option<i32>,
    ) {
        self.inner.publish_diagnostics(uri, diags, version).await;
    }

    pub async fn config(&self) -> Config {
        let config = self
            .inner
            .configuration(vec![ConfigurationItem {
                scope_uri: None,
                section: Some(BTLS_SECTION.to_string()),
            }])
            .await
            .ok()
            .filter(|c| c.len() == 1)
            .and_then(|configs| configs.into_iter().next())
            .unwrap_or_default();
        Config::from_value(config)
    }
}
