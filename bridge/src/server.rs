//! WebSocket server on 127.0.0.1. Checks Host and Origin during the handshake, requires a
//! valid hello (or a Figma pairing request) within a few seconds, then hands messages to
//! the hub.

use crate::hub::{AuthError, Hub, Origin};
use futures_util::{SinkExt, StreamExt};
use pulley_core::protocol::{self, Body, MAX_FRAME_BYTES, Role};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::{Response as HttpResponse, StatusCode};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tracing::{debug, info, warn};

const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CONNECTIONS: usize = 32;

pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
    // Loopback only, never 0.0.0.0.
    TcpListener::bind(("127.0.0.1", port)).await
}

/// Decide whether to accept a handshake: Host must name this server on loopback (stops DNS
/// rebinding), Origin must be a Pulley client (stops ordinary web pages).
pub fn check_request(req: &Request, port: u16) -> Result<Origin, &'static str> {
    let host = req.headers().get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
    let ok_host = host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}");
    if !ok_host {
        return Err("bad host");
    }
    match req.headers().get("origin").and_then(|h| h.to_str().ok()) {
        Some("file://") => Ok(Origin::Cep),
        Some("null") => Ok(Origin::Null),
        _ => Err("origin not allowed"),
    }
}

fn forbidden(why: &str) -> ErrorResponse {
    let mut r = HttpResponse::new(Some(why.to_string()));
    *r.status_mut() = StatusCode::FORBIDDEN;
    r
}

pub async fn run(listener: TcpListener, hub: Arc<Hub>) {
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    info!(port, "listening on 127.0.0.1");
    {
        let hub = hub.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                hub.cleanup();
            }
        });
    }
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                warn!("accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        if !peer.ip().is_loopback() {
            warn!(%peer, "refused non-loopback connection");
            continue;
        }
        if hub.connection_count() >= MAX_CONNECTIONS {
            warn!("too many connections; refusing");
            continue;
        }
        let hub = hub.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, port, hub).await {
                debug!("connection ended: {e}");
            }
        });
    }
}

async fn handle(stream: TcpStream, port: u16, hub: Arc<Hub>) -> Result<(), String> {
    let mut origin = None;
    // The callback type is fixed by tungstenite (its error type is an HTTP response).
    #[allow(clippy::result_large_err)]
    let cb = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        match check_request(req, port) {
            Ok(o) => {
                origin = Some(o);
                Ok(resp)
            }
            Err(why) => {
                warn!(why, "refused WebSocket handshake");
                Err(forbidden(why))
            }
        }
    };
    let config = WebSocketConfig::default().max_message_size(Some(MAX_FRAME_BYTES)).max_frame_size(Some(MAX_FRAME_BYTES));
    let ws = timeout(AUTH_TIMEOUT, tokio_tungstenite::accept_hdr_async_with_config(stream, cb, Some(config)))
        .await
        .map_err(|_| "handshake timeout".to_string())?
        .map_err(|e| e.to_string())?;
    let origin = origin.ok_or("no origin")?;
    let (mut sink, mut stream) = ws.split();
    let (tx, mut rx) = mpsc::channel::<String>(512);
    let writer = tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            if sink.send(WsMsg::text(text)).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });
    let send_now = |tx: &mpsc::Sender<String>, body: Body| {
        let _ = tx.try_send(protocol::to_text(&crate::config::random_hex(8), &body));
    };
    let send_err = |tx: &mpsc::Sender<String>, e: &AuthError| {
        let _ = tx.try_send(protocol::to_text(
            &crate::config::random_hex(8),
            &Body::Error { code: e.code.into(), message: e.message.clone(), hint: e.hint.clone(), reply_to: None },
        ));
    };
    let conn = hub.new_conn_id();
    let mut role: Option<Role> = None;
    let auth_deadline = Instant::now() + AUTH_TIMEOUT;
    loop {
        let next = if role.is_none() { tokio::time::timeout_at(auth_deadline, stream.next()).await } else { timeout(IDLE_TIMEOUT, stream.next()).await };
        let msg = match next {
            Err(_) => {
                debug!(conn, "timed out");
                break;
            }
            Ok(None) | Ok(Some(Err(_))) => break,
            Ok(Some(Ok(m))) => m,
        };
        let text = match msg {
            WsMsg::Text(t) => t.to_string(),
            WsMsg::Close(_) => break,
            WsMsg::Ping(_) | WsMsg::Pong(_) | WsMsg::Frame(_) => continue,
            WsMsg::Binary(_) => {
                send_err(&tx, &AuthError { code: "invalid_message", message: "Binary messages are not supported.".into(), hint: String::new() });
                break;
            }
        };
        let parsed = protocol::parse_message(&text);
        let m = match parsed {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.try_send(protocol::to_text(
                    &crate::config::random_hex(8),
                    &Body::Error { code: e.code.as_str().into(), message: e.message.clone(), hint: e.hint.clone(), reply_to: None },
                ));
                if role.is_none() {
                    break;
                }
                continue;
            }
        };
        match role {
            None => match m.body {
                Body::Hello { role: r, client_version, app_version, instance, token } => match hub.authenticate(origin, r, &token) {
                    Ok(()) => {
                        role = Some(r);
                        send_now(&tx, Body::Welcome { bridge_version: pulley_core::PULLEY_VERSION.into(), instance: format!("c{conn}") });
                        hub.register(conn, r, client_version, app_version, instance, tx.clone());
                    }
                    Err(e) => {
                        warn!(conn, code = e.code, "authentication failed");
                        send_err(&tx, &e);
                        // Slow down guessing.
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        break;
                    }
                },
                Body::PairRequest { code, name } if origin == Origin::Null => match hub.pair(&code, &name) {
                    Ok(secret) => send_now(&tx, Body::Paired { secret }),
                    Err(e) => {
                        send_err(&tx, &e);
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        break;
                    }
                },
                _ => {
                    send_err(&tx, &AuthError { code: "hello_required", message: "A client must say hello first.".into(), hint: String::new() });
                    break;
                }
            },
            Some(_) => {
                let id = m.id.clone();
                hub.handle(conn, &id, m.body).await;
            }
        }
    }
    hub.unregister(conn);
    drop(tx);
    let _ = writer.await;
    Ok(())
}
