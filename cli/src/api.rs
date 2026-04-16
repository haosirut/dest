//! Minimal HTTP API server for the daemon.
//!
//! Uses raw `tokio::net::TcpListener` for zero-dependency HTTP handling.
//! Supports JSON responses for status, balance, peers, and chunks endpoints.

use anyhow::Result;
use std::net::SocketAddr;
use tracing::{info, warn};

/// Run the API server on the given address.
///
/// Handles incoming HTTP requests and dispatches to the appropriate handler.
/// Each request is processed in a separate task for concurrency.
pub async fn run_api_server(addr: &str) -> Result<()> {
    let socket_addr: SocketAddr = addr
        .parse()
        .with_context(|| format!("Invalid listen address: {}", addr))?;

    let listener = tokio::net::TcpListener::bind(socket_addr).await?;

    info!("API server listening on {}", socket_addr);
    info!("Endpoints:");
    info!("  GET /api/v1/status  - Node status");
    info!("  GET /api/v1/balance - Account balance");
    info!("  GET /api/v1/peers   - Connected peers");
    info!("  GET /api/v1/chunks  - Stored chunks");

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                warn!("Error handling connection from {}: {}", peer_addr, e);
            }
        });
    }
}

use anyhow::Context;

/// Handle a single TCP connection (one HTTP request).
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let (status_code, body) = route_request(&request);

    let response = format_http_response(status_code, &body);
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;

    Ok(())
}

/// Route an HTTP request to the appropriate handler.
fn route_request(request: &str) -> (u16, String) {
    let path = extract_path(request);

    if path.starts_with("/api/v1/status") {
        handle_status()
    } else if path.starts_with("/api/v1/balance") {
        handle_balance()
    } else if path.starts_with("/api/v1/peers") {
        handle_peers()
    } else if path.starts_with("/api/v1/chunks") {
        handle_chunks()
    } else {
        (404, json_response("error", "not_found", &serde_json::json!({})))
    }
}

/// Extract the request path from an HTTP request string.
fn extract_path(request: &str) -> &str {
    // HTTP request format: "GET /path HTTP/1.1\r\n..."
    let lines: Vec<&str> = request.lines().collect();
    if lines.is_empty() {
        return "/";
    }
    let first_line = lines[0];
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1]
    } else {
        "/"
    }
}

// ---------------------------------------------------------------------------
// Endpoint handlers
// ---------------------------------------------------------------------------

fn handle_status() -> (u16, String) {
    let data = serde_json::json!({
        "status": "running",
        "version": env!("CARGO_PKG_VERSION"),
        "platform": vaultkeeper_storage::platform_type(),
        "hosting_available": vaultkeeper_storage::is_hosting_available(),
        "uptime": "0s"
    });
    (200, json_response("ok", "status", &data))
}

fn handle_balance() -> (u16, String) {
    let billing = vaultkeeper_billing::BillingEngine::new();
    let data = serde_json::json!({
        "balance": billing.get_current_balance().to_string(),
        "currency": "RUB",
        "subscription": format!("{:?}", billing.get_subscription()),
        "frozen": billing.is_frozen()
    });
    (200, json_response("ok", "balance", &data))
}

fn handle_peers() -> (u16, String) {
    let data = serde_json::json!({
        "peers": [],
        "count": 0
    });
    (200, json_response("ok", "peers", &data))
}

fn handle_chunks() -> (u16, String) {
    let data = serde_json::json!({
        "chunks": [],
        "count": 0
    });
    (200, json_response("ok", "chunks", &data))
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

/// Build a JSON response body with status and message fields.
fn json_response(status: &str, message: &str, data: &serde_json::Value) -> String {
    let body = serde_json::json!({
        "status": status,
        "message": message,
        "data": data
    });
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| {
        format!(r#"{{"status":"{}","message":"{}"}}"#, status, message)
    })
}

/// Format a full HTTP response with the given status code and body.
fn format_http_response(status_code: u16, body: &str) -> String {
    let reason = match status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         Access-Control-Allow-Origin: *\r\n\
         \r\n\
         {}",
        status_code,
        reason,
        body.len(),
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_get() {
        let request = "GET /api/v1/status HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(extract_path(request), "/api/v1/status");
    }

    #[test]
    fn test_extract_path_with_query() {
        let request = "GET /api/v1/balance?format=json HTTP/1.1\r\n\r\n";
        assert_eq!(extract_path(request), "/api/v1/balance?format=json");
    }

    #[test]
    fn test_extract_path_empty() {
        assert_eq!(extract_path(""), "/");
    }

    #[test]
    fn test_route_status() {
        let request = "GET /api/v1/status HTTP/1.1\r\n\r\n";
        let (code, body) = route_request(request);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\": \"ok\""));
        assert!(body.contains("\"version\""));
    }

    #[test]
    fn test_route_balance() {
        let request = "GET /api/v1/balance HTTP/1.1\r\n\r\n";
        let (code, body) = route_request(request);
        assert_eq!(code, 200);
        assert!(body.contains("\"status\": \"ok\""));
        assert!(body.contains("\"balance\""));
    }

    #[test]
    fn test_route_peers() {
        let request = "GET /api/v1/peers HTTP/1.1\r\n\r\n";
        let (code, body) = route_request(request);
        assert_eq!(code, 200);
        assert!(body.contains("\"count\": 0"));
    }

    #[test]
    fn test_route_chunks() {
        let request = "GET /api/v1/chunks HTTP/1.1\r\n\r\n";
        let (code, body) = route_request(request);
        assert_eq!(code, 200);
        assert!(body.contains("\"count\": 0"));
    }

    #[test]
    fn test_route_404() {
        let request = "GET /nonexistent HTTP/1.1\r\n\r\n";
        let (code, _body) = route_request(request);
        assert_eq!(code, 404);
    }

    #[test]
    fn test_format_http_response() {
        let body = r#"{"status":"ok"}"#;
        let response = format_http_response(200, body);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Content-Type: application/json"));
        assert!(response.contains(&format!("Content-Length: {}", body.len())));
        assert!(response.ends_with(body));
    }

    #[test]
    fn test_json_response() {
        let data = serde_json::json!({"key": "value"});
        let body = json_response("ok", "test", &data);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["message"], "test");
        assert_eq!(parsed["data"]["key"], "value");
    }

    #[test]
    fn test_handle_status_includes_platform() {
        let (code, body) = handle_status();
        assert_eq!(code, 200);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(parsed["data"]["platform"].is_string());
        assert!(parsed["data"]["hosting_available"].is_boolean());
    }
}
