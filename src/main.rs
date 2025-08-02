mod ast;
mod server;

#[tokio::main]
async fn main() {
    server::run().await;
}
