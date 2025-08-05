mod builtins;
mod completion_provider;
mod diagnostic_provider;
mod parser;
mod server;

#[tokio::main]
async fn main() {
    server::run().await;
}
