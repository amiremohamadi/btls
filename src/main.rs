mod builtins;
mod client;
mod completion_provider;
mod config;
mod diagnostic_provider;
mod parser;
mod server;

#[tokio::main]
async fn main() {
    server::run().await;
}
