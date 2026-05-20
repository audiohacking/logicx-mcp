//! Sidecar HTTP proxy: listens on a Unix socket and forwards to Ollama (local or remote).
//! Runs outside Logic Pro so AU sandbox never touches the network.

use logicx_core::ollama_proxy::{self, ProxyRequest, ProxyResponse};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::io::ErrorKind;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::{Command, Output, Stdio};
use std::thread;

fn main() {
    let dir = ollama_proxy::support_dir();
    let _ = fs::create_dir_all(&dir);

    let socket = ollama_proxy::socket_path();
    let _ = fs::remove_file(&socket);

    let listener = match UnixListener::bind(&socket) {
        Ok(l) => l,
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            // Another proxy instance is already serving this socket.
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("logicx-ollama-proxy: bind {} failed: {e}", socket.display());
            std::process::exit(1);
        }
    };

    let _ = fs::write(ollama_proxy::pid_path(), std::process::id().to_string());

    eprintln!(
        "logicx-ollama-proxy: listening on {} (pid {})",
        socket.display(),
        std::process::id()
    );

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                thread::spawn(|| {
                    let _ = handle_client(s);
                });
            }
            Err(e) => eprintln!("logicx-ollama-proxy: accept error: {e}"),
        }
    }

    let _ = fs::remove_file(&socket);
    let _ = fs::remove_file(ollama_proxy::pid_path());
}

fn handle_client(mut stream: UnixStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let response = match serde_json::from_str::<ProxyRequest>(line.trim()) {
        Ok(req) => dispatch(req),
        Err(e) => ProxyResponse::failure(format!("invalid request JSON: {e}")),
    };

    let out = serde_json::to_string(&response).unwrap_or_else(|e| {
        format!(r#"{{"ok":false,"error":"encode error: {e}"}}"#)
    });
    stream.write_all(out.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn dispatch(req: ProxyRequest) -> ProxyResponse {
    match req {
        ProxyRequest::Ping => ProxyResponse::success("pong"),
        ProxyRequest::HttpGet { url } => match curl_get(&url) {
            Ok(body) => ProxyResponse::success(body),
            Err(e) => ProxyResponse::failure(e),
        },
        ProxyRequest::HttpPost { url, body } => match curl_post(&url, &body) {
            Ok(resp) => ProxyResponse::success(resp),
            Err(e) => ProxyResponse::failure(e),
        },
    }
}

fn curl_get(url: &str) -> Result<String, String> {
    let output = Command::new("/usr/bin/curl")
        .args(["-sfS", "--max-time", "30", url])
        .output()
        .map_err(|e| format!("curl spawn failed: {e}"))?;
    curl_output(output)
}

fn curl_post(url: &str, json_body: &str) -> Result<String, String> {
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "-sfS",
            "--max-time",
            "300",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            "@-",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl spawn failed: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(json_body.as_bytes())
            .map_err(|e| e.to_string())?;
    }

    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    curl_output(output)
}

fn curl_output(output: Output) -> Result<String, String> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!(
        "curl exit {}: {stderr}",
        output.status.code().unwrap_or(-1)
    ))
}
