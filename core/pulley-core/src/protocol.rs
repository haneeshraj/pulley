//! Bridge protocol (JSON over WebSocket), shared by the bridge (native) and the clients
//! (through WebAssembly), so both sides parse messages with the same strict rules.
//!
//! Every message carries `protocol`, `id` and `type`. Unknown fields are rejected. Large
//! scenes travel as `scene_chunk` messages and are reassembled with a hard size cap.
//!
//! Parsing is two-step: a cheap envelope probe reads `protocol` and `type`, then the text is
//! parsed again into the strict struct for that type. (serde can't combine `flatten`,
//! internally tagged enums and `deny_unknown_fields`, and buffering would break the raw
//! scene passthrough.)

use crate::error::{Error, ErrorCode, Result};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::BTreeMap;

/// Protocol version. Bump on any incompatible change.
pub const PROTOCOL_VERSION: u32 = 1;
/// Largest single WebSocket message accepted (chunks are smaller than this).
pub const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024;
/// Chunk size clients and the bridge use for scene text.
pub const CHUNK_BYTES: usize = 512 * 1024;
/// Longest id/transfer/instance string.
pub const MAX_SHORT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Ae,
    Illustrator,
    Figma,
}

impl Role {
    pub fn component(self) -> &'static str {
        match self {
            Role::Ae => "Pulley After Effects panel",
            Role::Illustrator => "Pulley Illustrator panel",
            Role::Figma => "Pulley Figma plugin",
        }
    }
}

/// A connected client as reported in `status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientInfo {
    pub role: Role,
    pub client_version: String,
    pub app_version: String,
    pub instance: String,
    /// Most recently active client of its role (the one pushes and pulls go to).
    pub active: bool,
}

/// Result of an import in After Effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportSummary {
    pub ok: bool,
    /// Layers added.
    #[serde(default)]
    pub layers: u32,
    /// Existing layers from an earlier push that were updated in place.
    #[serde(default)]
    pub updated: u32,
    #[serde(default)]
    pub groups: u32,
    #[serde(default)]
    pub paths: u32,
    #[serde(default)]
    pub texts: u32,
    #[serde(default)]
    pub missing_fonts: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub cancelled: bool,
    /// Milliseconds per stage.
    #[serde(default)]
    pub timings: BTreeMap<String, f64>,
}

/// A parsed message body.
#[derive(Debug)]
pub enum Body {
    Hello { role: Role, client_version: String, app_version: String, instance: String, token: String },
    Welcome { bridge_version: String, instance: String },
    PairRequest { code: String, name: String },
    Paired { secret: String },
    Status { clients: Vec<ClientInfo> },
    PushScene { transfer: String, scene: Box<RawValue>, pull: bool },
    SceneChunk { transfer: String, index: u32, total: u32, data: String, pull: bool },
    PullRequest { transfer: String },
    PullError { transfer: String, code: String, message: String, hint: String },
    ImportResult { transfer: String, result: ImportSummary },
    Progress { transfer: String, done: u32, total: u32, stage: String },
    Cancel { transfer: String },
    Activity,
    Ping,
    Pong,
    Error { code: String, message: String, hint: String, reply_to: Option<String> },
}

#[derive(Debug)]
pub struct Message {
    pub id: String,
    pub body: Body,
}

/// Wire structs: one per message type, each with the common fields and nothing else.
mod wire {
    use super::*;

    macro_rules! wire {
        ($name:ident { $($(#[$a:meta])* $f:ident : $t:ty),* $(,)? }) => {
            #[derive(Deserialize, Serialize)]
            #[serde(deny_unknown_fields, rename_all = "camelCase")]
            pub struct $name {
                pub protocol: u32,
                pub id: String,
                #[serde(rename = "type")]
                pub kind: String,
                $($(#[$a])* pub $f: $t,)*
            }
        };
    }

    wire!(Hello {
        role: Role,
        client_version: String,
        #[serde(default)]
        app_version: String,
        #[serde(default)]
        instance: String,
        token: String
    });
    wire!(Welcome {
        bridge_version: String,
        #[serde(default)]
        instance: String
    });
    wire!(PairRequest {
        code: String,
        #[serde(default)]
        name: String
    });
    wire!(Paired { secret: String });
    wire!(Status { clients: Vec<ClientInfo> });
    wire!(PushScene { transfer: String, scene: Box<RawValue>, #[serde(default)] pull: bool });
    wire!(SceneChunk {
        transfer: String,
        index: u32,
        total: u32,
        data: String,
        #[serde(default)]
        pull: bool
    });
    wire!(PullRequest { transfer: String });
    wire!(PullError {
        transfer: String,
        code: String,
        message: String,
        #[serde(default)]
        hint: String
    });
    wire!(ImportResult { transfer: String, result: ImportSummary });
    wire!(Progress {
        transfer: String,
        done: u32,
        total: u32,
        #[serde(default)]
        stage: String
    });
    wire!(Cancel { transfer: String });
    wire!(Empty {});
    wire!(Error { code: String, message: String, #[serde(default)] hint: String, #[serde(default)] reply_to: Option<String> });
}

fn short_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_SHORT && s.bytes().all(|b| b.is_ascii_graphic())
}

fn malformed(e: serde_json::Error) -> Error {
    Error::data(ErrorCode::InvalidMessage, format!("A message from a Pulley client is malformed ({e})."))
}

/// Parse and check one message. The protocol version is checked first so a mismatch is
/// reported as such, naming the side to update.
pub fn parse_message(text: &str) -> Result<Message> {
    if text.len() > MAX_FRAME_BYTES {
        return Err(Error::data(ErrorCode::LimitExceeded, "A message from a Pulley client is too large."));
    }
    #[derive(Deserialize)]
    struct Probe {
        #[serde(default)]
        protocol: Option<u64>,
        #[serde(default, rename = "type")]
        kind: Option<String>,
    }
    let probe: Probe =
        serde_json::from_str(text).map_err(|e| Error::data(ErrorCode::InvalidMessage, format!("A message from a Pulley client is not valid JSON ({e}).")))?;
    match probe.protocol {
        Some(v) if v == u64::from(PROTOCOL_VERSION) => {}
        Some(v) => {
            let (msg, hint) = if v > u64::from(PROTOCOL_VERSION) {
                (format!("A Pulley client speaks protocol v{v}, newer than this bridge (v{PROTOCOL_VERSION})."), "Update the Pulley bridge.")
            } else {
                (
                    format!("A Pulley client speaks protocol v{v}, older than this bridge (v{PROTOCOL_VERSION})."),
                    "Update the Pulley panel or plugin that is connecting.",
                )
            };
            return Err(Error::new(ErrorCode::ProtocolMismatch, msg, hint));
        }
        None => return Err(Error::data(ErrorCode::InvalidMessage, "A message has no protocol version.")),
    }
    let kind = probe.kind.unwrap_or_default();
    fn p<'a, T: Deserialize<'a>>(t: &'a str) -> Result<T> {
        serde_json::from_str(t).map_err(malformed)
    }
    let (id, body) = match kind.as_str() {
        "hello" => {
            let w: wire::Hello = p(text)?;
            (w.id, Body::Hello { role: w.role, client_version: w.client_version, app_version: w.app_version, instance: w.instance, token: w.token })
        }
        "welcome" => {
            let w: wire::Welcome = p(text)?;
            (w.id, Body::Welcome { bridge_version: w.bridge_version, instance: w.instance })
        }
        "pair_request" => {
            let w: wire::PairRequest = p(text)?;
            (w.id, Body::PairRequest { code: w.code, name: w.name })
        }
        "paired" => {
            let w: wire::Paired = p(text)?;
            (w.id, Body::Paired { secret: w.secret })
        }
        "status" => {
            let w: wire::Status = p(text)?;
            (w.id, Body::Status { clients: w.clients })
        }
        "push_scene" => {
            let w: wire::PushScene = p(text)?;
            (w.id, Body::PushScene { transfer: w.transfer, scene: w.scene, pull: w.pull })
        }
        "scene_chunk" => {
            let w: wire::SceneChunk = p(text)?;
            (w.id, Body::SceneChunk { transfer: w.transfer, index: w.index, total: w.total, data: w.data, pull: w.pull })
        }
        "pull_request" => {
            let w: wire::PullRequest = p(text)?;
            (w.id, Body::PullRequest { transfer: w.transfer })
        }
        "pull_error" => {
            let w: wire::PullError = p(text)?;
            (w.id, Body::PullError { transfer: w.transfer, code: w.code, message: w.message, hint: w.hint })
        }
        "import_result" => {
            let w: wire::ImportResult = p(text)?;
            (w.id, Body::ImportResult { transfer: w.transfer, result: w.result })
        }
        "progress" => {
            let w: wire::Progress = p(text)?;
            (w.id, Body::Progress { transfer: w.transfer, done: w.done, total: w.total, stage: w.stage })
        }
        "cancel" => {
            let w: wire::Cancel = p(text)?;
            (w.id, Body::Cancel { transfer: w.transfer })
        }
        "activity" | "ping" | "pong" => {
            let w: wire::Empty = p(text)?;
            let b = match kind.as_str() {
                "activity" => Body::Activity,
                "ping" => Body::Ping,
                _ => Body::Pong,
            };
            (w.id, b)
        }
        "error" => {
            let w: wire::Error = p(text)?;
            (w.id, Body::Error { code: w.code, message: w.message, hint: w.hint, reply_to: w.reply_to })
        }
        _ => return Err(Error::data(ErrorCode::InvalidMessage, "A message has an unknown type.")),
    };
    if !short_ok(&id) {
        return Err(Error::data(ErrorCode::InvalidMessage, "A message has an invalid id."));
    }
    let m = Message { id, body };
    check(&m)?;
    Ok(m)
}

fn check(m: &Message) -> Result<()> {
    let transfer = match &m.body {
        Body::PushScene { transfer, .. }
        | Body::SceneChunk { transfer, .. }
        | Body::PullRequest { transfer }
        | Body::PullError { transfer, .. }
        | Body::ImportResult { transfer, .. }
        | Body::Progress { transfer, .. }
        | Body::Cancel { transfer } => Some(transfer),
        _ => None,
    };
    if let Some(t) = transfer
        && !short_ok(t)
    {
        return Err(Error::data(ErrorCode::InvalidMessage, "A message has an invalid transfer id."));
    }
    match &m.body {
        Body::SceneChunk { index, total, data, .. } => {
            let max_chunks = (crate::limits::MAX_MESSAGE_BYTES / CHUNK_BYTES + 1) as u32;
            if *total == 0 || *total > max_chunks || index >= total || data.len() > CHUNK_BYTES {
                return Err(Error::data(ErrorCode::InvalidMessage, "A scene chunk is out of range."));
            }
        }
        Body::Hello { client_version, app_version, instance, token, .. } => {
            if client_version.len() > 64 || app_version.len() > 64 || instance.len() > MAX_SHORT || token.len() > 256 {
                return Err(Error::data(ErrorCode::InvalidMessage, "A hello message has oversized fields."));
            }
        }
        Body::PairRequest { code, name } if code.len() > 32 || name.len() > 128 => {
            return Err(Error::data(ErrorCode::InvalidMessage, "A pairing request has oversized fields."));
        }
        Body::ImportResult { result, .. }
            if result.warnings.len() > 10_000 || result.errors.len() > 10_000 || result.missing_fonts.len() > 10_000 || result.timings.len() > 64 =>
        {
            return Err(Error::data(ErrorCode::InvalidMessage, "An import result is too large."));
        }
        _ => {}
    }
    Ok(())
}

/// Serialize a message.
pub fn to_text(id: &str, body: &Body) -> String {
    let base = |kind: &str| (PROTOCOL_VERSION, id.to_string(), kind.to_string());
    macro_rules! ser {
        ($w:ident, $kind:expr, { $($f:ident : $v:expr),* $(,)? }) => {{
            let (protocol, id, kind) = base($kind);
            serde_json::to_string(&wire::$w { protocol, id, kind, $($f: $v),* }).unwrap_or_default()
        }};
    }
    match body {
        Body::Hello { role, client_version, app_version, instance, token } => {
            ser!(Hello, "hello", { role: *role, client_version: client_version.clone(), app_version: app_version.clone(), instance: instance.clone(), token: token.clone() })
        }
        Body::Welcome { bridge_version, instance } => ser!(Welcome, "welcome", { bridge_version: bridge_version.clone(), instance: instance.clone() }),
        Body::PairRequest { code, name } => ser!(PairRequest, "pair_request", { code: code.clone(), name: name.clone() }),
        Body::Paired { secret } => ser!(Paired, "paired", { secret: secret.clone() }),
        Body::Status { clients } => ser!(Status, "status", { clients: clients.clone() }),
        Body::PushScene { transfer, scene, pull } => ser!(PushScene, "push_scene", { transfer: transfer.clone(), scene: scene.clone(), pull: *pull }),
        Body::SceneChunk { transfer, index, total, data, pull } => {
            ser!(SceneChunk, "scene_chunk", { transfer: transfer.clone(), index: *index, total: *total, data: data.clone(), pull: *pull })
        }
        Body::PullRequest { transfer } => ser!(PullRequest, "pull_request", { transfer: transfer.clone() }),
        Body::PullError { transfer, code, message, hint } => {
            ser!(PullError, "pull_error", { transfer: transfer.clone(), code: code.clone(), message: message.clone(), hint: hint.clone() })
        }
        Body::ImportResult { transfer, result } => ser!(ImportResult, "import_result", { transfer: transfer.clone(), result: result.clone() }),
        Body::Progress { transfer, done, total, stage } => {
            ser!(Progress, "progress", { transfer: transfer.clone(), done: *done, total: *total, stage: stage.clone() })
        }
        Body::Cancel { transfer } => ser!(Cancel, "cancel", { transfer: transfer.clone() }),
        Body::Activity => ser!(Empty, "activity", {}),
        Body::Ping => ser!(Empty, "ping", {}),
        Body::Pong => ser!(Empty, "pong", {}),
        Body::Error { code, message, hint, reply_to } => {
            ser!(Error, "error", { code: code.clone(), message: message.clone(), hint: hint.clone(), reply_to: reply_to.clone() })
        }
    }
}

/// Split scene JSON text into chunks of at most [`CHUNK_BYTES`] bytes on char boundaries.
pub fn chunk(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + CHUNK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        out.push(&text[start..end]);
        start = end;
    }
    if out.is_empty() {
        out.push("");
    }
    out
}

/// Reassembles the chunks of one transfer, with a total size cap.
#[derive(Debug, Default)]
pub struct Reassembly {
    pub total: u32,
    pub next: u32,
    pub text: String,
}

impl Reassembly {
    /// Add a chunk; returns the full text when the last chunk arrives. Chunks must be in order.
    pub fn push(&mut self, index: u32, total: u32, data: &str) -> Result<Option<String>> {
        if self.next == 0 {
            self.total = total;
        }
        if total != self.total || index != self.next {
            return Err(Error::data(ErrorCode::InvalidMessage, "Scene chunks arrived out of order."));
        }
        if self.text.len() + data.len() > crate::limits::MAX_MESSAGE_BYTES {
            return Err(Error::new(
                ErrorCode::LimitExceeded,
                "The scene is larger than the 64 MB limit.",
                "Send a smaller selection, or split it into several pushes.",
            ));
        }
        self.text.push_str(data);
        self.next += 1;
        if self.next == self.total {
            return Ok(Some(std::mem::take(&mut self.text)));
        }
        Ok(None)
    }
}

/// Constant-time comparison for tokens and secrets.
pub fn secret_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrip_and_strictness() {
        let t = r#"{"protocol":1,"id":"m1","type":"hello","role":"ae","clientVersion":"0.1.0","appVersion":"25.5","instance":"i1","token":"abc"}"#;
        let m = parse_message(t).unwrap();
        assert!(matches!(m.body, Body::Hello { role: Role::Ae, .. }));
        let back = to_text(&m.id, &m.body);
        assert!(parse_message(&back).is_ok());
        let extra = t.replace("\"token\"", "\"evil\":1,\"token\"");
        assert_eq!(parse_message(&extra).unwrap_err().code, ErrorCode::InvalidMessage);
        let unknown_type = r#"{"protocol":1,"id":"m1","type":"rm_rf"}"#;
        assert!(parse_message(unknown_type).is_err());
        let ping_extra = r#"{"protocol":1,"id":"m1","type":"ping","x":1}"#;
        assert!(parse_message(ping_extra).is_err());
    }

    #[test]
    fn protocol_version_errors_name_the_side_to_update() {
        let e = parse_message(r#"{"protocol":2,"id":"m","type":"ping"}"#).unwrap_err();
        assert_eq!(e.code, ErrorCode::ProtocolMismatch);
        assert!(e.hint.contains("bridge"));
        let e = parse_message(r#"{"protocol":0,"id":"m","type":"ping"}"#).unwrap_err();
        assert!(e.hint.contains("panel or plugin"));
    }

    #[test]
    fn push_scene_keeps_raw_scene_text() {
        let t = r#"{"protocol":1,"id":"m","type":"push_scene","transfer":"t1","scene":{"schema":"pulley-scene","version":1}}"#;
        let m = parse_message(t).unwrap();
        match &m.body {
            Body::PushScene { scene, transfer, pull } => {
                assert_eq!(transfer, "t1");
                assert!(!pull);
                assert_eq!(scene.get(), r#"{"schema":"pulley-scene","version":1}"#);
            }
            _ => panic!(),
        }
        let back = to_text("m", &m.body);
        assert!(back.contains(r#""scene":{"schema":"pulley-scene","version":1}"#), "{back}");
    }

    #[test]
    fn chunks_and_reassembly() {
        let text: String = "é".repeat(CHUNK_BYTES);
        let parts = chunk(&text);
        assert!(parts.len() >= 2 && parts.iter().all(|p| p.len() <= CHUNK_BYTES));
        let mut r = Reassembly::default();
        let total = parts.len() as u32;
        let mut done = None;
        for (i, p) in parts.iter().enumerate() {
            done = r.push(i as u32, total, p).unwrap();
        }
        assert_eq!(done.unwrap(), text);
        let mut bad = Reassembly::default();
        assert!(bad.push(1, 3, "x").is_err());
    }

    #[test]
    fn chunk_bounds_are_checked() {
        assert!(parse_message(r#"{"protocol":1,"id":"m","type":"scene_chunk","transfer":"t","index":5,"total":3,"data":""}"#).is_err());
        assert!(parse_message(r#"{"protocol":1,"id":"m","type":"scene_chunk","transfer":"t","index":0,"total":100000,"data":""}"#).is_err());
    }

    #[test]
    fn ids_must_be_short_printable() {
        assert!(parse_message("{\"protocol\":1,\"id\":\"a b\",\"type\":\"ping\"}").is_err());
        let long = format!("{{\"protocol\":1,\"id\":\"{}\",\"type\":\"ping\"}}", "x".repeat(200));
        assert!(parse_message(&long).is_err());
    }

    #[test]
    fn constant_time_eq() {
        assert!(secret_eq("abc", "abc"));
        assert!(!secret_eq("abc", "abd"));
        assert!(!secret_eq("abc", "ab"));
    }

    #[test]
    fn serializes_every_kind() {
        let bodies = vec![
            Body::Welcome { bridge_version: "0.1.0".into(), instance: "x".into() },
            Body::Status { clients: vec![] },
            Body::PullRequest { transfer: "t".into() },
            Body::Progress { transfer: "t".into(), done: 1, total: 2, stage: "apply".into() },
            Body::ImportResult { transfer: "t".into(), result: ImportSummary { ok: true, ..Default::default() } },
            Body::Activity,
            Body::Ping,
            Body::Pong,
            Body::Error { code: "no_ae".into(), message: "m".into(), hint: "h".into(), reply_to: Some("y".into()) },
        ];
        for b in bodies {
            let t = to_text("x", &b);
            assert!(t.contains("\"protocol\":1"), "{t}");
            assert!(parse_message(&t).is_ok(), "{t}");
        }
    }
}
