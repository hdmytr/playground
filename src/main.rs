use axum::{
    routing::get,
    Router,
};

use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    
    init_tracing();
    
    let http_addr = "0.0.0.0:3000"; 

    let http_listener = TcpListener::bind(http_addr).await?;
    tracing::info!("starting HTTP server on http://{}", http_addr);

    let app = app();

    loop {
        let (stream, peer_addr) = http_listener.accept().await?;
        tracing::info!("new connection from {}", peer_addr);

        let app = app.clone();
        tokio::spawn(async move {
            if let Err(err) = hyper::server::conn::Http::new()
                .serve_connection(stream, app)
                .await
            {
                tracing::error!("error HTTP connection from {}: {}", peer_addr, err);
            }
        });
    }
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

fn app() -> Router {
    Router::new().route("/", get(handler))
}

async fn handler() -> &'static str {
    tracing::info!("GET on root route");
    "Hello and welcome!"
}
