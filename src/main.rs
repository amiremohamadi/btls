mod ast;
mod builtins;
mod completion_provider;
mod server;

#[tokio::main]
async fn main() {
    server::run().await;
}
