use axum::{
    routing::get,
    Router,
};

use tokio_rustls::rustls::{Certificate, PrivateKey, ServerConfig};
use tokio_rustls::TlsAcceptor;

use std::{fs::File, io::BufReader, sync::Arc};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// TODO: 
//      - deploy with github actions on (vm / GitHub Codespaces / )
//      - add / manage trusted certs for production
//      - add main page and host it on  GitHub Pages
//      - add info route with uptime, version (commit hash), logs, etc.

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {

    init_tracing();

    let tls_config = Arc::new(load_tls_config()?);
    let tls_acceptor = TlsAcceptor::from(tls_config);

    let http_addr = "0.0.0.0:3000";
    let https_addr = "0.0.0.0:3001";

    let http_listener = TcpListener::bind(http_addr).await?;
    tracing::info!("starting HTTP server on http://{}", http_addr);

    let https_listener = TcpListener::bind(https_addr).await?;
    tracing::info!("starting HTTPS server on  https://{}", https_addr);

    let main_app = app();
    let http_app = main_app.clone();
    let https_app = main_app.clone();
    
    let http_handle = tokio::spawn(async move {
        loop {
            let (stream, peer_addr) = http_listener.accept().await.unwrap();
            let app = http_app.clone();
            if let Err(err) = hyper::server::conn::Http::new()
                .serve_connection(stream, app)
                .await
            {
                tracing::error!("error on HTTP connection from {}: {}", peer_addr, err);
            }
        }
    });
    

    let https_handle = tokio::spawn(async move {
        loop {
            let (stream, peer_addr) = https_listener.accept().await.unwrap();
            let acceptor = tls_acceptor.clone();

            let app = https_app.clone();
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    if let Err(err) = hyper::server::conn::Http::new()
                        .serve_connection(tls_stream, app)
                        .await
                    {
                        tracing::error!("error on HTTPS connection from {}: {}", peer_addr, err);
                    }
                }
                Err(err) => {
                    tracing::error!("TLS handshake error: {}", err);
                }
            }
        }
    });

    tokio::try_join!(http_handle, https_handle)?;

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}


fn load_tls_config() -> Result<ServerConfig, anyhow::Error> {

    let certs = load_certs("certs/cert.pem")?;
    let key = load_private_key("certs/key.pem")?;

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
        anyhow::bail!("no valid private keys found in {}", path);
    }
    Ok(PrivateKey(keys[0].clone()))
}


fn app() -> Router {
    Router::new().route("/", get(handler))
}

async fn handler() -> &'static str {
    tracing::info!("GET on root route");
    "Hello and welcome!"
}