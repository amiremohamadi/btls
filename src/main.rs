mod analyzer;
mod builtins;
mod client;
mod common;
mod completion_provider;
mod config;
mod diagnostic_provider;
mod parser;
mod server;
mod storage;

#[tokio::main]
async fn main() {
    server::run().await;
}
