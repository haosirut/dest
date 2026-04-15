//! Minimal HTTP API server for the daemon.

use anyhow::Result;
use std::net::SocketAddr;
use tracing::info;

/// Run a minimal HTTP API server.
/// In production, this would use axum/actix-web for proper routing.
pub async fn run_api_server(addr: &str) -> Result<()> {
    let socket_addr: SocketAddr = addr.parse()?;

    info!("API server listening on {}", socket_addr);
    info!("Available endpoints:");
    info!("  GET  /api/v1/status    - Node status");
    info!("  GET  /api/v1/balance   - Account balance");
    info!("  POST /api/v1/upload    - Upload file");
    info!("  GET  /api/v1/download  - Download file");
    info!("  GET  /api/v1/peers     - Connected peers");
    info!("  GET  /api/v1/chunks    - Stored chunks");

    // Minimal TCP listener for the API
    // In production, replace with axum/actix-web router
    let listener = tokio::net::TcpListener::bind(socket_addr).await?;
    
    loop {
        let (mut stream, addr) = listener.accept().await?;
        info!("API connection from: {}", addr);

        // Read the request
        let mut buf = [0u8; 4096];
        let n = match tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
            Ok(0) => continue,
            Ok(n) => n,
            Err(_) => continue,
        };

        let request = String::from_utf8_lossy(&buf[..n]);
        let response = handle_request(&request);

        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await?;
        tokio::io::AsyncWriteExt::shutdown(&mut stream).await?;
    }
}

fn handle_request(request: &str) -> String {
    let status_response = r#"{"status":"running","version":"0.1.0"}"#;

    let response = if request.contains("GET /api/v1/status") {
        status_response
    } else if request.contains("GET /api/v1/balance") {
        r#"{"balance":"0.00","currency":"RUB","status":"active"}"#
    } else if request.contains("GET /api/v1/peers") {
        r#"{"peers":[],"count":0}"#
    } else if request.contains("GET /api/v1/chunks") {
        r#"{"chunks":[],"count":0}"#
    } else {
        r#"{"error":"not_found"}"#
    };

    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.len(),
        response
    )
}
