use axum::{
    routing::get,
    Router,
    Json,
};
use tower_http::cors::{Any, CorsLayer};
use serde::Serialize;
use std::{
    fs::File,
    io::BufReader,
    net::SocketAddr,
    sync::Arc,
    time::Instant,
};

use tokio::{
    net::TcpListener,
    signal,
    sync::{Notify, Semaphore},
};
use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};
use tokio_rustls::TlsAcceptor;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    init_tracing();

    let start_time = Instant::now();

    let https_addr = std::env::var("HTTPS_ADDR").unwrap_or_else(|_| "0.0.0.0:3001".to_string());
    let https_addr: SocketAddr = https_addr.parse()?;

    let main_app = app(start_time);

    let concurrency_str = std::env::var("MAX_CONCURRENCY").unwrap_or_else(|_| "100".to_string());
    let concurrency_num = concurrency_str.parse::<usize>().unwrap_or(100);
    let concurrency_limit = Arc::new(Semaphore::new(concurrency_num));

    let tls_config = Arc::new(load_tls_config()?);
    let tls_acceptor = TlsAcceptor::from(tls_config);

    let https_listener = TcpListener::bind(https_addr).await?;
    tracing::info!("Starting HTTPS server on https://{}", https_addr);

    let concurrency_clone = concurrency_limit.clone();
    let app_clone = main_app.clone();
    tokio::spawn(async move {
        loop {
            let (stream, peer_addr) = match https_listener.accept().await {
                Ok(conn) => conn,
                Err(err) => {
                    tracing::error!("TLS accept error: {}", err);
                    break;
                }
            };

            let concurrency_clone = concurrency_clone.clone();
            let acceptor = tls_acceptor.clone();
            let app_clone = app_clone.clone();
            tokio::spawn(async move {
                match concurrency_clone.acquire_owned().await {
                    Ok(permit) => {
                        let _permit = permit;
                        match acceptor.accept(stream).await {
                            Ok(tls_stream) => {
                                if let Err(err) = hyper::server::conn::Http::new()
                                    .serve_connection(tls_stream, app_clone)
                                    .await
                                {
                                    tracing::error!(
                                        "Error on HTTPS connection from {}: {}",
                                        peer_addr, err
                                    );
                                }
                            }
                            Err(err) => {
                                tracing::error!(
                                    "TLS handshake error from {}: {}",
                                    peer_addr, err
                                );
                            }
                        }
                    }
                    Err(err) => {
                        tracing::error!(
                            "HTTPS concurrency semaphore error: {}",
                            err
                        );
                    }
                }
            });
        }
    });


    let shutdown_notify = Arc::new(Notify::new());
    let handle = tokio::spawn(async move {
        if let Err(err) = signal::ctrl_c().await {
            tracing::error!("Failed to listen for ctrl+c: {}", err);
        }
        tracing::info!("Shutdown signal received");
        shutdown_notify.notify_waiters();
    });

    tokio::select! {
        _ = handle => {
            tracing::info!("Shutting down gracefully");
        }
    }

    Ok(())
}


fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}


fn load_tls_config() -> Result<ServerConfig, anyhow::Error> {
    let cert_path = std::env::var("TLS_CERT_PATH").unwrap_or_else(|_| "certs/cert.pem".to_string());
    let key_path = std::env::var("TLS_KEY_PATH").unwrap_or_else(|_| "certs/key.pem".to_string());

    let certs = load_certs(&cert_path)?;
    let key = load_private_key(&key_path)?;

    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(config)
}

fn load_certs(path: &str) -> Result<Vec<Certificate>, anyhow::Error> {
    let cert_file = File::open(path)?;
    let mut reader = BufReader::new(cert_file);
    let certs = rustls_pemfile::certs(&mut reader)?
        .into_iter()
        .map(Certificate)
        .collect();
    Ok(certs)
}

fn load_private_key(path: &str) -> Result<PrivateKey, anyhow::Error> {
    let key_file = File::open(path)?;
    let mut reader = BufReader::new(key_file);
    let keys = rustls_pemfile::pkcs8_private_keys(&mut reader)?;
    if keys.is_empty() {
        anyhow::bail!("No valid private keys found in {}", path);
    }
    Ok(PrivateKey(keys[0].clone()))
}

// Global application state
#[derive(Clone)]
struct AppState {
    start_time: Instant,
}


fn app(start_time: Instant) -> Router {
    let state = AppState { start_time };
    Router::new()
        .route("/info", get(info_handler))
        .route("/ping", get(|| async { "OK" }))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

#[derive(Serialize)]
struct UptimeMs {
    uptime_ms: u128,
}

async fn info_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<UptimeMs> {
    let elapsed = state.start_time.elapsed();
    let uptime_ms = elapsed.as_millis();
    Json(UptimeMs { uptime_ms })
}
