use anyhow::{Context, Result};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub async fn serve(addr: &str) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding healthcheck listener on {addr}"))?;
    let started = Instant::now();
    tracing::info!(addr, "healthcheck listening");
    loop {
        let (mut socket, _) = listener.accept().await?;
        let started = started;
        tokio::spawn(async move {
            let mut buffer = [0u8; 1024];
            let read = match socket.read(&mut buffer).await {
                Ok(read) => read,
                Err(_) => return,
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request.split_whitespace().nth(1).unwrap_or("/");
            let response = handle(&path, started);
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        });
    }
}

fn handle(path: &str, started: Instant) -> String {
    if path == "/health" {
        let body = format!(
            "{{\"status\":\"ok\",\"uptime_secs\":{}}}",
            started.elapsed().as_secs()
        );
        http_response(200, "OK", &body)
    } else {
        http_response(404, "Not Found", "{\"status\":\"not found\"}")
    }
}

fn http_response(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener as TokioListener;

    fn bind_ephemeral() -> (String, TokioListener) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let addr = listener.local_addr().unwrap().to_string();
        let listener = TokioListener::from_std(listener).unwrap();
        (addr, listener)
    }

    #[tokio::test]
    async fn health_returns_200_with_json_body() {
        let (addr, listener) = bind_ephemeral();
        let started = Instant::now();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 1024];
            let read = socket.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let response = handle(&path, started);
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert!(body["uptime_secs"].as_u64().is_some());

        server.await.unwrap();
    }

    #[tokio::test]
    async fn unknown_path_returns_404() {
        let (addr, listener) = bind_ephemeral();
        let started = Instant::now();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 1024];
            let read = socket.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let response = handle(&path, started);
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/nope"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 404);

        server.await.unwrap();
    }
}
