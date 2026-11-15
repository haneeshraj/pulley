//! Development tool: a stand-in After Effects client. Receives scenes through the bridge,
//! summarizes them, answers with an import_result, and can ask Illustrator for a Pull.
//! cargo run -p pulley-bridge --example fake_ae -- [--port 47290] [--pulls N] [--expect N] [--out DIR]
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port: u16 = arg(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(47290);
    let mut pulls: u32 = arg(&args, "--pulls").and_then(|p| p.parse().ok()).unwrap_or(0);
    let expect: u32 = arg(&args, "--expect").and_then(|p| p.parse().ok()).unwrap_or(1);
    let out = arg(&args, "--out").map(std::path::PathBuf::from);
    let config = directories::BaseDirs::new().expect("dirs").config_dir().join("Pulley");
    let token = std::fs::read_to_string(config.join("session.token")).expect("session.token");
    let mut req = format!("ws://127.0.0.1:{port}/").into_client_request().unwrap();
    req.headers_mut().insert("Origin", HeaderValue::from_static("file://"));
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("connect");
    let hello = serde_json::json!({"protocol":1,"id":"h","type":"hello","role":"ae","clientVersion":"0.1.0","appVersion":"fake-ae","instance":"fake","token":token.trim()});
    ws.send(Message::text(hello.to_string())).await.unwrap();
    let mut got = 0;
    let mut chunks: std::collections::HashMap<String, String> = Default::default();
    let mut pull_sent_at: Option<Instant> = None;
    let deadline = Instant::now() + Duration::from_secs(900);
    while got < expect {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(Some(Ok(Message::Text(t)))) = tokio::time::timeout(left, ws.next()).await else {
            eprintln!("timed out after {got} scene(s)");
            std::process::exit(1);
        };
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        match v["type"].as_str() {
            Some("status") => {
                let ai = v["clients"].as_array().is_some_and(|c| c.iter().any(|c| c["role"] == "illustrator"));
                println!("status: illustrator {}", if ai { "connected" } else { "not connected" });
                if ai && pulls > 0 && pull_sent_at.is_none() {
                    // Give Illustrator's test driver time to open and select a fixture.
                    tokio::time::sleep(Duration::from_secs(25)).await;
                    pulls -= 1;
                    pull_sent_at = Some(Instant::now());
                    let m = serde_json::json!({"protocol":1,"id":"q","type":"pull_request","transfer":format!("pull{pulls}")});
                    ws.send(Message::text(m.to_string())).await.unwrap();
                    println!("sent pull request");
                }
            }
            Some("push_scene") | Some("scene_chunk") => {
                let transfer = v["transfer"].as_str().unwrap_or("").to_string();
                let text = if v["type"] == "push_scene" {
                    v["scene"].to_string()
                } else {
                    let e = chunks.entry(transfer.clone()).or_default();
                    e.push_str(v["data"].as_str().unwrap_or(""));
                    if v["index"].as_u64().unwrap_or(0) + 1 < v["total"].as_u64().unwrap_or(1) {
                        continue;
                    }
                    chunks.remove(&transfer).unwrap_or_default()
                };
                got += 1;
                let scene: serde_json::Value = serde_json::from_str(&text).unwrap();
                let names: Vec<String> = scene["items"].as_array().map(|a| a.iter().take(8).map(|i| i["name"].as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
                let pull = v["pull"].as_bool().unwrap_or(false);
                println!(
                    "scene {got} ({}): {} bytes, {} top-level items {:?}, {} warnings{}",
                    if pull { "pull" } else { "push" },
                    text.len(),
                    scene["items"].as_array().map(Vec::len).unwrap_or(0),
                    names,
                    scene["warnings"].as_array().map(Vec::len).unwrap_or(0),
                    pull_sent_at.filter(|_| pull).map(|t| format!(", pull answered in {:?}", t.elapsed())).unwrap_or_default()
                );
                if let Some(dir) = &out {
                    let _ = std::fs::create_dir_all(dir);
                    let _ = std::fs::write(dir.join(format!("scene-{got}.json")), &text);
                }
                let r = serde_json::json!({"protocol":1,"id":format!("r{got}"),"type":"import_result","transfer":transfer,"result":{"ok":true,"layers":scene["items"].as_array().map(Vec::len).unwrap_or(0)}});
                ws.send(Message::text(r.to_string())).await.unwrap();
            }
            Some("pull_error") | Some("error") => println!("error: {v}"),
            _ => {}
        }
    }
}
