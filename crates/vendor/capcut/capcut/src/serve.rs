use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;

/// Tiny sync HTTP server that serves the draft's JSON at GET /draft.
/// Uses only std::net::TcpListener (no axum). Intended for `capcut-cli serve` demo.
pub fn serve_draft(draft: &Path, port: u16) -> Result<()> {
    serve_draft_on(draft, "127.0.0.1", port)
}

pub fn serve_draft_on(draft: &Path, host: &str, port: u16) -> Result<()> {
    let draft_path = crate::store::resolve_draft_path(draft)?;
    serve_listener(host, port, draft_path)
}

pub fn serve_hint_on(host: &str, port: u16) -> Result<()> {
    // No draft provided — serve a hint at / and 404 at /draft
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).with_context(|| format!("bind {}", addr))?;
    eprintln!("[serve] listening on http://{} — GET /draft (no draft bound, hint only)", addr);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                thread::spawn(move || {
                    let _ = handle_hint_client(s);
                });
            }
            Err(e) => eprintln!("[serve] accept error: {}", e),
        }
    }
    Ok(())
}

/// Async entry expected by CLI `Serve { queue, port, host }` (queue = draft path when serving draft).
pub async fn serve(queue: Option<PathBuf>, host: &str, port: u16) -> Result<()> {
    if let Some(q) = queue {
        // `queue` is a draft path in the demo serve; resolve and serve it
        serve_draft_on(&q, host, port)
    } else {
        serve_hint_on(host, port)
    }
}

fn serve_listener(host: &str, port: u16, draft_path: PathBuf) -> Result<()> {
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).with_context(|| format!("bind {}", addr))?;
    eprintln!("[serve] listening on http://{} — GET /draft", addr);
    eprintln!("[serve] draft: {}", draft_path.display());
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let dp = draft_path.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_client(s, &dp) {
                        eprintln!("[serve] client error: {:#}", e);
                    }
                });
            }
            Err(e) => eprintln!("[serve] accept error: {}", e),
        }
    }
    Ok(())
}

fn handle_hint_client(mut stream: TcpStream) -> Result<()> {
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).context("read request")?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    if first_line.starts_with("GET / ") || first_line.starts_with("GET / HTTP") {
        let body = br#"{"ok":true,"hint":"GET /draft - no draft bound; start with capcut serve --queue <draft>"}"#;
        write_response(&mut stream, 200, "OK", "application/json", body)?;
    } else if first_line.starts_with("GET /draft") {
        let body = br#"{"error":"no draft bound"}"#;
        write_response(&mut stream, 404, "Not Found", "application/json", body)?;
    } else {
        let body = br#"{"error":"not found"}"#;
        write_response(&mut stream, 404, "Not Found", "application/json", body)?;
    }
    Ok(())
}

fn handle_client(mut stream: TcpStream, draft_path: &PathBuf) -> Result<()> {
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).context("read request")?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    let is_get_draft = first_line.starts_with("GET /draft")
        || first_line.starts_with("GET /draft ")
        || first_line == "GET /draft HTTP/1.1"
        || first_line.starts_with("GET /draft?");

    if is_get_draft {
        let body = std::fs::read(draft_path)
            .with_context(|| format!("read draft {}", draft_path.display()))?;
        write_response(&mut stream, 200, "OK", "application/json", &body)?;
    } else if first_line.starts_with("GET / ") || first_line.starts_with("GET / HTTP") {
        let body = br#"{"ok":true,"hint":"GET /draft"}"#;
        write_response(&mut stream, 200, "OK", "application/json", body)?;
    } else {
        let body = br#"{"error":"not found"}"#;
        write_response(&mut stream, 404, "Not Found", "application/json", body)?;
    }
    Ok(())
}

fn write_response(stream: &mut TcpStream, code: u16, text: &str, ctype: &str, body: &[u8]) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        code, text, ctype, body.len()
    );
    stream.write_all(header.as_bytes()).context("write header")?;
    stream.write_all(body).context("write body")?;
    stream.flush().context("flush")?;
    Ok(())
}
