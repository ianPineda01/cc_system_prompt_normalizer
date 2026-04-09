use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use regex::Regex;
use std::net::SocketAddr;
use std::sync::Arc;

const LM_STUDIO: &str = "http://127.0.0.1:1234";

fn normalize_system_prompt(text: &str) -> String {
    let re = Regex::new(r"cch=[0-9a-fA-F]+;").unwrap();
    re.replace_all(text, "cch=0;").to_string()
}

fn normalize_body(body: &[u8]) -> Result<Vec<u8>, serde_json::Error> {
    let mut json: serde_json::Value = serde_json::from_slice(body)?;

    // Anthropic format: top-level "system" field
    if let Some(system) = json.get_mut("system") {
        match system {
            serde_json::Value::String(s) => {
                *s = normalize_system_prompt(s);
            }
            serde_json::Value::Array(blocks) => {
                for block in blocks.iter_mut() {
                    if let Some(text) = block.get_mut("text").and_then(|t| t.as_str().map(String::from)) {
                        block["text"] = serde_json::Value::String(normalize_system_prompt(&text));
                    }
                }
            }
            _ => {}
        }
    }

    // OpenAI format: messages with role "system"
    if let Some(messages) = json.get_mut("messages").and_then(|m| m.as_array_mut()) {
        for msg in messages.iter_mut() {
            if msg.get("role").and_then(|r| r.as_str()) == Some("system") {
                if let Some(content) = msg.get_mut("content").and_then(|c| c.as_str().map(String::from)) {
                    msg["content"] = serde_json::Value::String(normalize_system_prompt(&content));
                }
            }
        }
    }

    serde_json::to_vec(&json)
}

async fn proxy(State(verbose): State<Arc<bool>>, req: Request) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/");

    println!("[proxy] {} {}", method, path);

    // Block HEAD requests — Claude Code sends these on /clear but LM Studio doesn't handle them
    if method == axum::http::Method::HEAD {
        return Ok(StatusCode::OK.into_response());
    }

    let client = reqwest::Client::new();
    let verbose = *verbose;
    let url = format!("{LM_STUDIO}{path}");

    // Collect headers, skip host
    let mut headers = HeaderMap::new();
    for (key, val) in req.headers() {
        if key != "host" && key != "content-length" {
            headers.insert(key.clone(), val.clone());
        }
    }

    // Read and normalize body
    let body_bytes = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let normalized = if !body_bytes.is_empty() {
        normalize_body(&body_bytes).unwrap_or_else(|_| body_bytes.to_vec())
    } else {
        body_bytes.to_vec()
    };

    // Forward to LM Studio
    if verbose {
        println!("[proxy] Forwarding to {url} ({} bytes)", normalized.len());
        for (key, val) in &headers {
            println!("[proxy]   {}: {}", key, val.to_str().unwrap_or("<binary>"));
        }
    }
    let resp = client
        .request(method, &url)
        .headers(headers)
        .body(normalized)
        .send()
        .await
        .map_err(|e| {
            eprintln!("[proxy] Error forwarding request: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    // Build response
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let resp_headers = resp.headers().clone();
    let resp_bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    let mut response = (status, Body::from(resp_bytes)).into_response();
    for (key, val) in resp_headers.iter() {
        if key != "transfer-encoding" {
            response
                .headers_mut()
                .insert(key.clone(), val.clone());
        }
    }

    Ok(response)
}

#[tokio::main]
async fn main() {
    let verbose = Arc::new(std::env::args().any(|a| a == "--verbose-log"));
    let app = Router::new().fallback(any(proxy)).with_state(verbose);

    let addr = SocketAddr::from(([127, 0, 0, 1], 7609));
    println!("Proxy listening on http://{addr}");
    println!("Forwarding to {LM_STUDIO} (normalizing cch= in system prompts)");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
