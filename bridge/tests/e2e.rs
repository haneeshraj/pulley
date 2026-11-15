//! End-to-end tests: start the real bridge binary headless on a free port with a private
//! config folder, then talk to it the way the panels and the Figma plugin do.

use futures_util::{SinkExt, StreamExt};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Bridge {
    child: Child,
    port: u16,
    dir: PathBuf,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn start(extra: &[&str]) -> Bridge {
    let port = free_port();
    let dir = std::env::temp_dir().join(format!("pulley-e2e-{port}"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut args = vec!["--headless".to_string(), "--stderr".into(), "--port".into(), port.to_string(), "--config-dir".into(), dir.display().to_string()];
    args.extend(extra.iter().map(|s| s.to_string()));
    let child = Command::new(env!("CARGO_BIN_EXE_pulley-bridge")).args(&args).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(start.elapsed() < Duration::from_secs(10), "bridge did not start");
        std::thread::sleep(Duration::from_millis(50));
    }
    Bridge { child, port, dir }
}

impl Bridge {
    fn token(&self) -> String {
        std::fs::read_to_string(self.dir.join("session.token")).unwrap()
    }
}

async fn connect_with(port: u16, host: &str, origin: Option<&str>) -> Result<Ws, String> {
    let mut req = format!("ws://127.0.0.1:{port}/").into_client_request().unwrap();
    req.headers_mut().insert("Host", HeaderValue::from_str(host).unwrap());
    if let Some(o) = origin {
        req.headers_mut().insert("Origin", HeaderValue::from_str(o).unwrap());
    }
    tokio_tungstenite::connect_async(req).await.map(|(ws, _)| ws).map_err(|e| e.to_string())
}

async fn connect(port: u16, origin: &str) -> Ws {
    connect_with(port, &format!("127.0.0.1:{port}"), Some(origin)).await.unwrap()
}

async fn send(ws: &mut Ws, v: serde_json::Value) {
    ws.send(Message::text(v.to_string())).await.unwrap();
}

/// Next message of the given type (skipping status/progress noise), with a timeout.
async fn recv_type(ws: &mut Ws, kind: &str) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let msg = tokio::time::timeout(left, ws.next()).await.unwrap_or_else(|_| panic!("timeout waiting for {kind}"));
        let msg = msg.expect("closed").expect("ws error");
        if let Message::Text(t) = msg {
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            if v["type"] == kind {
                return v;
            }
            if v["type"] == "error" && kind != "error" {
                panic!("unexpected error while waiting for {kind}: {v}");
            }
        }
    }
}

async fn hello(ws: &mut Ws, role: &str, token: &str) -> serde_json::Value {
    send(ws, serde_json::json!({"protocol":1,"id":"h1","type":"hello","role":role,"clientVersion":"0.1.0","appVersion":"test","instance":role,"token":token}))
        .await;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let msg = tokio::time::timeout(left, ws.next()).await.expect("timeout").expect("closed").expect("ws");
        if let Message::Text(t) = msg {
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            if v["type"] == "welcome" || v["type"] == "error" {
                return v;
            }
        }
    }
}

fn scene_text(name: &str) -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../schema/examples").join(name)).unwrap()
}

#[tokio::test]
async fn handshake_rejects_bad_host_and_origin() {
    let b = start(&[]);
    let port = b.port;
    // DNS rebinding: a page on evil.example resolving to 127.0.0.1 sends its own Host.
    assert!(connect_with(port, &format!("evil.example:{port}"), Some("file://")).await.is_err());
    // An ordinary web page.
    assert!(connect_with(port, &format!("127.0.0.1:{port}"), Some("https://evil.example")).await.is_err());
    // No Origin at all.
    assert!(connect_with(port, &format!("127.0.0.1:{port}"), None).await.is_err());
    // Allowed origins connect (but still need a hello).
    assert!(connect_with(port, &format!("localhost:{port}"), Some("file://")).await.is_ok());
    assert!(connect_with(port, &format!("127.0.0.1:{port}"), Some("null")).await.is_ok());
}

#[tokio::test]
async fn hello_requires_the_session_token_and_matching_origin() {
    let b = start(&[]);
    let mut ws = connect(b.port, "file://").await;
    let r = hello(&mut ws, "ae", "wrong").await;
    assert_eq!(r["code"], "bad_token");
    // Right token, wrong origin for the role.
    let mut ws = connect(b.port, "null").await;
    let r = hello(&mut ws, "ae", &b.token()).await;
    assert_eq!(r["code"], "wrong_origin");
    let mut ws = connect(b.port, "file://").await;
    let r = hello(&mut ws, "ae", &b.token()).await;
    assert_eq!(r["type"], "welcome");
    // Figma without pairing.
    let mut f = connect(b.port, "null").await;
    let r = hello(&mut f, "figma", &b.token()).await;
    assert_eq!(r["code"], "not_paired");
}

#[tokio::test]
async fn first_message_must_be_hello() {
    let b = start(&[]);
    let mut ws = connect(b.port, "file://").await;
    send(&mut ws, serde_json::json!({"protocol":1,"id":"x","type":"ping"})).await;
    let r = recv_type(&mut ws, "error").await;
    assert_eq!(r["code"], "hello_required");
}

#[tokio::test]
async fn push_from_illustrator_reaches_ae_and_result_comes_back() {
    let b = start(&[]);
    let token = b.token();
    let mut ae = connect(b.port, "file://").await;
    assert_eq!(hello(&mut ae, "ae", &token).await["type"], "welcome");
    let mut ai = connect(b.port, "file://").await;
    assert_eq!(hello(&mut ai, "illustrator", &token).await["type"], "welcome");
    let scene: serde_json::Value = serde_json::from_str(&scene_text("basic-shapes.json")).unwrap();
    let t0 = Instant::now();
    send(&mut ai, serde_json::json!({"protocol":1,"id":"p1","type":"push_scene","transfer":"t1","scene":scene})).await;
    let got = recv_type(&mut ae, "push_scene").await;
    let rtt = t0.elapsed();
    assert_eq!(got["transfer"], "t1");
    assert_eq!(got["scene"], scene);
    assert!(rtt < Duration::from_millis(50), "bridge round trip {rtt:?}");
    eprintln!("push round trip (basic scene): {rtt:?}");
    send(&mut ae, serde_json::json!({"protocol":1,"id":"r1","type":"import_result","transfer":"t1","result":{"ok":true,"layers":2}})).await;
    let res = recv_type(&mut ai, "import_result").await;
    assert_eq!(res["result"]["layers"], 2);
}

#[tokio::test]
async fn pull_from_ae_goes_to_illustrator_and_back() {
    let b = start(&[]);
    let token = b.token();
    let mut ae = connect(b.port, "file://").await;
    hello(&mut ae, "ae", &token).await;
    let mut ai = connect(b.port, "file://").await;
    hello(&mut ai, "illustrator", &token).await;
    send(&mut ae, serde_json::json!({"protocol":1,"id":"q","type":"pull_request","transfer":"pull1"})).await;
    let req = recv_type(&mut ai, "pull_request").await;
    assert_eq!(req["transfer"], "pull1");
    let scene: serde_json::Value = serde_json::from_str(&scene_text("clip-and-text.json")).unwrap();
    send(&mut ai, serde_json::json!({"protocol":1,"id":"a","type":"push_scene","transfer":"pull1","pull":true,"scene":scene})).await;
    let got = recv_type(&mut ae, "push_scene").await;
    assert_eq!(got["pull"], true);
    assert_eq!(got["scene"]["items"][0]["name"], "Clip Group");
}

#[tokio::test]
async fn pull_without_illustrator_and_push_without_ae_explain_what_to_do() {
    let b = start(&[]);
    let token = b.token();
    let mut ae = connect(b.port, "file://").await;
    hello(&mut ae, "ae", &token).await;
    send(&mut ae, serde_json::json!({"protocol":1,"id":"q","type":"pull_request","transfer":"p"})).await;
    let e = recv_type(&mut ae, "error").await;
    assert_eq!(e["code"], "no_source");
    assert!(e["hint"].as_str().unwrap().contains("Illustrator") && e["hint"].as_str().unwrap().contains("Figma"));
    drop(ae);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut ai = connect(b.port, "file://").await;
    hello(&mut ai, "illustrator", &token).await;
    let scene: serde_json::Value = serde_json::from_str(&scene_text("basic-shapes.json")).unwrap();
    send(&mut ai, serde_json::json!({"protocol":1,"id":"p","type":"push_scene","transfer":"t","scene":scene})).await;
    let e = recv_type(&mut ai, "error").await;
    assert_eq!(e["code"], "no_ae");
}

#[tokio::test]
async fn invalid_scenes_are_rejected_and_never_forwarded() {
    let b = start(&[]);
    let token = b.token();
    let mut ae = connect(b.port, "file://").await;
    hello(&mut ae, "ae", &token).await;
    let mut ai = connect(b.port, "file://").await;
    hello(&mut ai, "illustrator", &token).await;
    let mut scene: serde_json::Value = serde_json::from_str(&scene_text("basic-shapes.json")).unwrap();
    scene["items"][0]["evil"] = "x".into();
    send(&mut ai, serde_json::json!({"protocol":1,"id":"p","type":"push_scene","transfer":"bad","scene":scene})).await;
    let e = recv_type(&mut ai, "error").await;
    assert_eq!(e["code"], "invalid_scene");
    // AE got nothing but status updates.
    let got = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            if let Some(Ok(Message::Text(t))) = ae.next().await
                && (t.contains("push_scene") || t.contains("scene_chunk"))
            {
                return true;
            }
        }
    })
    .await;
    assert!(got.is_err(), "AE must not receive an invalid scene");
}

#[tokio::test]
async fn big_scenes_travel_in_chunks() {
    let b = start(&[]);
    let token = b.token();
    let mut ae = connect(b.port, "file://").await;
    hello(&mut ae, "ae", &token).await;
    let mut ai = connect(b.port, "file://").await;
    hello(&mut ai, "illustrator", &token).await;
    // ~1.5 MB scene: 3000 shapes.
    let base: serde_json::Value = serde_json::from_str(&scene_text("basic-shapes.json")).unwrap();
    let mut scene = base.clone();
    let shape = base["items"][0].clone();
    let items: Vec<serde_json::Value> = (0..3000)
        .map(|i| {
            let mut s = shape.clone();
            s["id"] = format!("s{i}").into();
            s
        })
        .collect();
    scene["items"] = items.into();
    let text = scene.to_string();
    let parts: Vec<&str> = pulley_chunks(&text);
    let t0 = Instant::now();
    for (i, p) in parts.iter().enumerate() {
        send(&mut ai, serde_json::json!({"protocol":1,"id":format!("c{i}"),"type":"scene_chunk","transfer":"big","index":i,"total":parts.len(),"data":p}))
            .await;
    }
    let mut got = String::new();
    loop {
        let c = recv_type(&mut ae, "scene_chunk").await;
        got.push_str(c["data"].as_str().unwrap());
        if c["index"].as_u64().unwrap() + 1 == c["total"].as_u64().unwrap() {
            break;
        }
    }
    eprintln!("{} bytes in {} chunks forwarded in {:?}", text.len(), parts.len(), t0.elapsed());
    let back: serde_json::Value = serde_json::from_str(&got).unwrap();
    assert_eq!(back, scene);
}

fn pulley_chunks(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = (start + 512 * 1024).min(text.len());
        out.push(&text[start..end]);
        start = end;
    }
    out
}

#[tokio::test]
async fn oversized_frames_close_the_connection() {
    let b = start(&[]);
    let mut ws = connect(b.port, "file://").await;
    hello(&mut ws, "ae", &b.token()).await;
    let huge = "x".repeat(3 * 1024 * 1024);
    let _ = ws.send(Message::text(huge)).await;
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match ws.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return true,
                _ => {}
            }
        }
    })
    .await;
    assert!(closed.unwrap_or(false));
}

#[tokio::test]
async fn protocol_mismatch_names_the_component() {
    let b = start(&[]);
    let mut ws = connect(b.port, "file://").await;
    send(&mut ws, serde_json::json!({"protocol":2,"id":"h","type":"hello","role":"ae","clientVersion":"9","token":"x"})).await;
    let e = recv_type(&mut ws, "error").await;
    assert_eq!(e["code"], "protocol_mismatch");
    assert!(e["hint"].as_str().unwrap().contains("bridge"));
}

#[tokio::test]
async fn figma_pairing_then_push() {
    let b = start(&["--start-pairing"]);
    let code = std::fs::read_to_string(b.dir.join("pairing-code.txt")).unwrap();
    let mut f = connect(b.port, "null").await;
    send(&mut f, serde_json::json!({"protocol":1,"id":"p","type":"pair_request","code":"WRONG-CODE","name":"Figma test"})).await;
    let e = recv_type(&mut f, "error").await;
    assert_eq!(e["code"], "pairing_failed");
    let mut f = connect(b.port, "null").await;
    send(&mut f, serde_json::json!({"protocol":1,"id":"p","type":"pair_request","code":code.to_lowercase(),"name":"Figma test"})).await;
    let paired = recv_type(&mut f, "paired").await;
    let secret = paired["secret"].as_str().unwrap().to_string();
    assert_eq!(secret.len(), 64);
    // The code is single-use.
    let mut again = connect(b.port, "null").await;
    send(&mut again, serde_json::json!({"protocol":1,"id":"p","type":"pair_request","code":code,"name":"x"})).await;
    assert_eq!(recv_type(&mut again, "error").await["code"], "pairing_failed");
    // The secret works as a Figma token (also after reconnecting).
    let mut f2 = connect(b.port, "null").await;
    assert_eq!(hello(&mut f2, "figma", &secret).await["type"], "welcome");
    let mut ae = connect(b.port, "file://").await;
    hello(&mut ae, "ae", &b.token()).await;
    let scene: serde_json::Value = serde_json::from_str(&scene_text("gradients-compound.json")).unwrap();
    send(&mut f2, serde_json::json!({"protocol":1,"id":"p","type":"push_scene","transfer":"fig1","scene":scene})).await;
    assert_eq!(recv_type(&mut ae, "push_scene").await["transfer"], "fig1");
}

#[tokio::test]
async fn pull_goes_to_the_source_app_used_last() {
    let b = start(&["--start-pairing"]);
    let code = std::fs::read_to_string(b.dir.join("pairing-code.txt")).unwrap();
    let token = b.token();
    let mut ai = connect(b.port, "file://").await;
    hello(&mut ai, "illustrator", &token).await;
    let mut f = connect(b.port, "null").await;
    send(&mut f, serde_json::json!({"protocol":1,"id":"p","type":"pair_request","code":code,"name":"Figma test"})).await;
    let secret = recv_type(&mut f, "paired").await["secret"].as_str().unwrap().to_string();
    let mut fig = connect(b.port, "null").await;
    hello(&mut fig, "figma", &secret).await;
    let mut ae = connect(b.port, "file://").await;
    hello(&mut ae, "ae", &token).await;
    // Figma connected after Illustrator: it answers.
    send(&mut ae, serde_json::json!({"protocol":1,"id":"q","type":"pull_request","transfer":"p1"})).await;
    assert_eq!(recv_type(&mut fig, "pull_request").await["transfer"], "p1");
    let scene: serde_json::Value = serde_json::from_str(&scene_text("basic-shapes.json")).unwrap();
    send(&mut fig, serde_json::json!({"protocol":1,"id":"a","type":"push_scene","transfer":"p1","pull":true,"scene":scene})).await;
    assert_eq!(recv_type(&mut ae, "push_scene").await["pull"], true);
    // Illustrator becomes active again: the next pull goes there.
    send(&mut ai, serde_json::json!({"protocol":1,"id":"a","type":"activity"})).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    send(&mut ae, serde_json::json!({"protocol":1,"id":"q2","type":"pull_request","transfer":"p2"})).await;
    assert_eq!(recv_type(&mut ai, "pull_request").await["transfer"], "p2");
}

#[tokio::test]
async fn most_recently_active_ae_gets_the_push() {
    let b = start(&[]);
    let token = b.token();
    let mut ae1 = connect(b.port, "file://").await;
    hello(&mut ae1, "ae", &token).await;
    let mut ae2 = connect(b.port, "file://").await;
    hello(&mut ae2, "ae", &token).await;
    // ae1 becomes active again.
    send(&mut ae1, serde_json::json!({"protocol":1,"id":"a","type":"activity"})).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut ai = connect(b.port, "file://").await;
    hello(&mut ai, "illustrator", &token).await;
    let scene: serde_json::Value = serde_json::from_str(&scene_text("basic-shapes.json")).unwrap();
    send(&mut ai, serde_json::json!({"protocol":1,"id":"p","type":"push_scene","transfer":"x","scene":scene})).await;
    assert_eq!(recv_type(&mut ae1, "push_scene").await["transfer"], "x");
    let status = recv_type(&mut ai, "status").await;
    let clients = status["clients"].as_array().unwrap();
    assert_eq!(clients.iter().filter(|c| c["role"] == "ae" && c["active"] == true).count(), 1);
}
