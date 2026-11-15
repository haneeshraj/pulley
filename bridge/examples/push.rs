//! Development tool: push a scene file through a running bridge as an "illustrator" client
//! and print the import result with timings.
//! cargo run -p pulley-bridge --example push -- <scene.json>... [--port 47290] [--config-dir DIR] [--wait-ae 120]
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port: u16 = arg(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(47290);
    let wait_ae: u64 = arg(&args, "--wait-ae").and_then(|p| p.parse().ok()).unwrap_or(120);
    let config =
        arg(&args, "--config-dir").map(std::path::PathBuf::from).unwrap_or_else(|| directories::BaseDirs::new().expect("dirs").config_dir().join("Pulley"));
    let scenes: Vec<String> =
        args.iter().enumerate().filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--"))).map(|(_, a)| a.clone()).collect();
    let token = std::fs::read_to_string(config.join("session.token")).expect("session.token (is the bridge running?)");
    let mut req = format!("ws://127.0.0.1:{port}/").into_client_request().unwrap();
    req.headers_mut().insert("Origin", HeaderValue::from_static("file://"));
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("connect");
    let hello = serde_json::json!({"protocol":1,"id":"h","type":"hello","role":"illustrator","clientVersion":"0.1.0","appVersion":"push-tool","instance":"push","token":token.trim()});
    ws.send(Message::text(hello.to_string())).await.unwrap();
    // Wait until an After Effects client is connected.
    let deadline = Instant::now() + Duration::from_secs(wait_ae);
    let mut ae_ready = false;
    while !ae_ready {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(Some(Ok(Message::Text(t)))) = tokio::time::timeout(left, ws.next()).await else {
            eprintln!("no After Effects client connected within {wait_ae} s");
            std::process::exit(1);
        };
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        if v["type"] == "error" {
            eprintln!("bridge error: {v}");
            std::process::exit(1);
        }
        if v["type"] == "status" {
            ae_ready = v["clients"].as_array().is_some_and(|c| c.iter().any(|c| c["role"] == "ae"));
        }
    }
    println!("After Effects is connected");
    for (n, file) in scenes.iter().enumerate() {
        let text = std::fs::read_to_string(file).expect("scene file");
        let transfer = format!("push{n}");
        let t0 = Instant::now();
        let chunk = 512 * 1024;
        if text.len() <= chunk {
            let head = serde_json::json!({"protocol":1,"id":format!("p{n}"),"type":"push_scene","transfer":transfer}).to_string();
            let msg = format!("{},\"scene\":{}}}", &head[..head.len() - 1], text);
            ws.send(Message::text(msg)).await.unwrap();
        } else {
            let parts: Vec<&str> = {
                let mut v = Vec::new();
                let mut s = 0;
                while s < text.len() {
                    let mut e = (s + chunk).min(text.len());
                    while !text.is_char_boundary(e) {
                        e -= 1;
                    }
                    v.push(&text[s..e]);
                    s = e;
                }
                v
            };
            for (i, p) in parts.iter().enumerate() {
                let m =
                    serde_json::json!({"protocol":1,"id":format!("c{n}-{i}"),"type":"scene_chunk","transfer":transfer,"index":i,"total":parts.len(),"data":p});
                ws.send(Message::text(m.to_string())).await.unwrap();
            }
        }
        let mut forwarded = None;
        loop {
            let Ok(Some(Ok(Message::Text(t)))) = tokio::time::timeout(Duration::from_secs(600), ws.next()).await else {
                eprintln!("{file}: no result within 10 minutes");
                std::process::exit(1);
            };
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            if v["transfer"] != transfer.as_str() && v["replyTo"] != transfer.as_str() {
                continue;
            }
            match v["type"].as_str() {
                Some("progress") if v["stage"] == "forwarded" => forwarded = Some(t0.elapsed()),
                Some("import_result") => {
                    println!("{file}: forwarded after {:?}, imported after {:?}", forwarded.unwrap_or_default(), t0.elapsed());
                    println!("  {}", v["result"]);
                    break;
                }
                Some("error") => {
                    println!("{file}: error {}", v);
                    break;
                }
                _ => {}
            }
        }
    }
}
