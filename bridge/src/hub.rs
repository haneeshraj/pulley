//! Connected clients, authentication, routing of pushes and pulls between them.

use crate::config::{self, FigmaClient, Paths, Settings};
use pulley_core::protocol::{self, Body, ClientInfo, Reassembly, Role, CHUNK_BYTES};
use pulley_core::validate::{parse_scene, Component};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

/// Where a connection came from, decided by its Origin header during the handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// CEP panels (After Effects, Illustrator): `Origin: file://`.
    Cep,
    /// Figma plugin iframe: `Origin: null`.
    Null,
}

pub type ConnId = u64;

struct Client {
    role: Role,
    client_version: String,
    app_version: String,
    instance: String,
    last_active: u64,
    tx: mpsc::Sender<String>,
    chunks: HashMap<String, Reassembly>,
}

struct Transfer {
    /// The client that started it: the pusher, or the AE instance that asked for a pull.
    requester: ConnId,
    /// The client that receives the scene (AE) or the pull request (Illustrator).
    target: ConnId,
    pull: bool,
    created: Instant,
}

struct Pairing {
    code: String,
    expires: Instant,
    attempts: u32,
}

/// Summary for the tray icon and menu.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub ae: usize,
    pub illustrator: usize,
    pub figma: usize,
    pub transferring: bool,
    pub last_error: Option<String>,
    pub pairing_code: Option<String>,
}

pub struct Hub {
    token: String,
    paths: Paths,
    settings: Mutex<Settings>,
    clients: Mutex<HashMap<ConnId, Client>>,
    transfers: Mutex<HashMap<String, Transfer>>,
    pairing: Mutex<Option<Pairing>>,
    next_conn: AtomicU64,
    activity: AtomicU64,
    summary: watch::Sender<Summary>,
}

pub struct AuthError {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
}

const PAIRING_TTL: Duration = Duration::from_secs(120);
const TRANSFER_TTL: Duration = Duration::from_secs(15 * 60);

impl Hub {
    pub fn new(token: String, paths: Paths, settings: Settings) -> (Hub, watch::Receiver<Summary>) {
        let (tx, rx) = watch::channel(Summary::default());
        (
            Hub {
                token,
                paths,
                settings: Mutex::new(settings),
                clients: Mutex::new(HashMap::new()),
                transfers: Mutex::new(HashMap::new()),
                pairing: Mutex::new(None),
                next_conn: AtomicU64::new(1),
                activity: AtomicU64::new(1),
                summary: tx,
            },
            rx,
        )
    }

    pub fn new_conn_id(&self) -> ConnId {
        self.next_conn.fetch_add(1, Ordering::Relaxed)
    }

    pub fn connection_count(&self) -> usize {
        self.clients.lock().map(|c| c.len()).unwrap_or(0)
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        if let Ok(mut s) = self.settings.lock() {
            f(&mut s);
            if let Err(e) = s.save(&self.paths) {
                warn!("could not save settings: {e}");
            }
        }
    }

    /// Check a hello: role must match the origin, and the token must be the session token
    /// (CEP) or a paired secret (Figma).
    pub fn authenticate(&self, origin: Origin, role: Role, token: &str) -> Result<(), AuthError> {
        let ok = match (origin, role) {
            (Origin::Cep, Role::Ae | Role::Illustrator) => protocol::secret_eq(token, &self.token),
            (Origin::Null, Role::Figma) => self
                .settings
                .lock()
                .map(|s| s.figma_clients.iter().any(|c| protocol::secret_eq(token, &c.secret)))
                .unwrap_or(false),
            _ => {
                return Err(AuthError {
                    code: "wrong_origin",
                    message: format!("A {} connected from an unexpected place.", role.component()),
                    hint: "Only Pulley's own panels and plugin can connect.".into(),
                });
            }
        };
        if ok {
            return Ok(());
        }
        Err(match role {
            Role::Figma => AuthError {
                code: "not_paired",
                message: "This Figma plugin is not paired with the Pulley bridge.".into(),
                hint: "Click the Pulley icon in the system tray (menu bar on macOS), choose \"Pair Figma plugin\", and enter the code in the plugin.".into(),
            },
            _ => AuthError {
                code: "bad_token",
                message: "The panel's session key does not match the running Pulley bridge.".into(),
                hint: "Click Reconnect. If that fails, quit and restart the Pulley bridge.".into(),
            },
        })
    }

    /// Start a pairing window and return the code to show.
    pub fn start_pairing(&self) -> String {
        let code = config::pairing_code();
        if let Ok(mut p) = self.pairing.lock() {
            *p = Some(Pairing { code: config::normalize_code(&code), expires: Instant::now() + PAIRING_TTL, attempts: 0 });
        }
        self.summary.send_modify(|s| s.pairing_code = Some(code.clone()));
        info!("pairing window opened for 2 minutes");
        code
    }

    /// Exchange a pairing code for a long-lived secret.
    pub fn pair(&self, code: &str, name: &str) -> Result<String, AuthError> {
        let mut guard = self.pairing.lock().map_err(|_| AuthError { code: "internal", message: "Pairing is unavailable.".into(), hint: String::new() })?;
        let fail = |message: &str| AuthError {
            code: "pairing_failed",
            message: message.into(),
            hint: "Choose \"Pair Figma plugin\" in the Pulley tray menu to get a new code, then enter it within 2 minutes.".into(),
        };
        let Some(p) = guard.as_mut() else { return Err(fail("No pairing code is active.")) };
        if Instant::now() > p.expires {
            *guard = None;
            self.summary.send_modify(|s| s.pairing_code = None);
            return Err(fail("The pairing code has expired."));
        }
        if !protocol::secret_eq(&config::normalize_code(code), &p.code) {
            p.attempts += 1;
            if p.attempts >= 5 {
                *guard = None;
                self.summary.send_modify(|s| s.pairing_code = None);
                warn!("pairing code invalidated after 5 wrong attempts");
            }
            return Err(fail("The pairing code is wrong."));
        }
        *guard = None;
        drop(guard);
        let secret = config::random_hex(32);
        let label: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
        self.update_settings(|s| {
            s.figma_clients.push(FigmaClient { name: if label.is_empty() { "Figma".into() } else { label }, secret: secret.clone(), created: config::now_stamp() });
            let n = s.figma_clients.len();
            if n > 16 {
                s.figma_clients.drain(..n - 16);
            }
        });
        self.summary.send_modify(|s| s.pairing_code = None);
        info!("Figma plugin paired");
        Ok(secret)
    }

    pub fn revoke_figma(&self) {
        self.update_settings(|s| s.figma_clients.clear());
        info!("all Figma pairings revoked");
    }

    pub fn register(&self, conn: ConnId, role: Role, client_version: String, app_version: String, instance: String, tx: mpsc::Sender<String>) {
        let last_active = self.activity.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut c) = self.clients.lock() {
            c.insert(conn, Client { role, client_version, app_version, instance, last_active, tx, chunks: HashMap::new() });
        }
        info!(conn, ?role, "client connected");
        self.broadcast_status();
    }

    pub fn unregister(&self, conn: ConnId) {
        let removed = self.clients.lock().ok().and_then(|mut c| c.remove(&conn));
        if let Some(c) = removed {
            info!(conn, role = ?c.role, "client disconnected");
            // Tell the other side of any open transfer.
            let open: Vec<(String, ConnId)> = self
                .transfers
                .lock()
                .map(|t| {
                    t.iter()
                        .filter(|(_, x)| x.requester == conn || x.target == conn)
                        .map(|(k, x)| (k.clone(), if x.requester == conn { x.target } else { x.requester }))
                        .collect()
                })
                .unwrap_or_default();
            for (transfer, other) in open {
                self.send(other, &Body::Error {
                    code: "peer_disconnected".into(),
                    message: format!("The {} disconnected during the transfer.", c.role.component()),
                    hint: "Check that the app is still open, then try again.".into(),
                    reply_to: Some(transfer.clone()),
                });
                if let Ok(mut t) = self.transfers.lock() {
                    t.remove(&transfer);
                }
            }
            self.broadcast_status();
        }
    }

    fn send(&self, conn: ConnId, body: &Body) -> bool {
        let tx = self.clients.lock().ok().and_then(|c| c.get(&conn).map(|c| c.tx.clone()));
        match tx {
            Some(tx) => tx.try_send(protocol::to_text(&config::random_hex(8), body)).is_ok(),
            None => false,
        }
    }

    pub fn send_error(&self, conn: ConnId, code: &str, message: &str, hint: &str, reply_to: Option<String>) {
        self.send(conn, &Body::Error { code: code.into(), message: message.into(), hint: hint.into(), reply_to });
    }

    fn most_recent(&self, role: Role) -> Option<ConnId> {
        self.clients.lock().ok()?.iter().filter(|(_, c)| c.role == role).max_by_key(|(_, c)| c.last_active).map(|(id, _)| *id)
    }

    fn role_of(&self, conn: ConnId) -> Option<Role> {
        self.clients.lock().ok()?.get(&conn).map(|c| c.role)
    }

    pub fn broadcast_status(&self) {
        let (list, txs): (Vec<ClientInfo>, Vec<mpsc::Sender<String>>) = match self.clients.lock() {
            Ok(c) => {
                let mut best: HashMap<Role, u64> = HashMap::new();
                for cl in c.values() {
                    let e = best.entry(cl.role).or_insert(0);
                    *e = (*e).max(cl.last_active);
                }
                let list = c
                    .values()
                    .map(|cl| ClientInfo {
                        role: cl.role,
                        client_version: cl.client_version.clone(),
                        app_version: cl.app_version.clone(),
                        instance: cl.instance.clone(),
                        active: best.get(&cl.role) == Some(&cl.last_active),
                    })
                    .collect();
                (list, c.values().map(|cl| cl.tx.clone()).collect())
            }
            Err(_) => return,
        };
        let count = |r: Role| list.iter().filter(|c| c.role == r).count();
        let (ae, ai, fg) = (count(Role::Ae), count(Role::Illustrator), count(Role::Figma));
        self.summary.send_modify(|s| {
            s.ae = ae;
            s.illustrator = ai;
            s.figma = fg;
        });
        let text = protocol::to_text(&config::random_hex(8), &Body::Status { clients: list });
        for tx in txs {
            let _ = tx.try_send(text.clone());
        }
    }

    pub fn note_error(&self, msg: &str) {
        let m = msg.to_string();
        self.summary.send_modify(|s| s.last_error = Some(m));
    }

    fn set_transferring(&self) {
        let busy = self.transfers.lock().map(|t| !t.is_empty()).unwrap_or(false);
        self.summary.send_modify(|s| s.transferring = busy);
    }

    pub fn cleanup(&self) {
        if let Ok(mut t) = self.transfers.lock() {
            t.retain(|_, x| x.created.elapsed() < TRANSFER_TTL);
        }
        if let Ok(mut p) = self.pairing.lock() {
            if p.as_ref().is_some_and(|p| Instant::now() > p.expires) {
                *p = None;
                self.summary.send_modify(|s| s.pairing_code = None);
            }
        }
        self.set_transferring();
    }

    /// Handle one message from an authenticated client.
    pub async fn handle(&self, conn: ConnId, id: &str, body: Body) {
        let Some(role) = self.role_of(conn) else { return };
        match body {
            Body::Ping => {
                self.send(conn, &Body::Pong);
            }
            Body::Pong => {}
            Body::Activity => {
                let n = self.activity.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut c) = self.clients.lock() {
                    if let Some(cl) = c.get_mut(&conn) {
                        cl.last_active = n;
                    }
                }
                self.broadcast_status();
            }
            Body::PushScene { transfer, scene, pull } => {
                self.process_scene(conn, role, id, transfer, scene.get().to_string(), pull).await;
            }
            Body::SceneChunk { transfer, index, total, data, pull } => {
                let done = {
                    let mut c = match self.clients.lock() {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    let Some(cl) = c.get_mut(&conn) else { return };
                    if !cl.chunks.contains_key(&transfer) && cl.chunks.len() >= 4 {
                        Err(pulley_core::Error::data(pulley_core::ErrorCode::LimitExceeded, "Too many transfers at once."))
                    } else {
                        let r = cl.chunks.entry(transfer.clone()).or_default();
                        let res = r.push(index, total, &data);
                        if !matches!(res, Ok(None)) {
                            cl.chunks.remove(&transfer);
                        }
                        res
                    }
                };
                match done {
                    Ok(Some(text)) => self.process_scene(conn, role, id, transfer, text, pull).await,
                    Ok(None) => {}
                    Err(e) => self.send_error(conn, e.code.as_str(), &e.message, &e.hint, Some(transfer)),
                }
            }
            Body::PullRequest { transfer } => {
                if role != Role::Ae {
                    return self.send_error(conn, "not_allowed", "Only After Effects can pull.", "", Some(transfer));
                }
                let Some(target) = self.most_recent(Role::Illustrator) else {
                    return self.send_error(
                        conn,
                        "no_illustrator",
                        "Illustrator is not connected to Pulley.",
                        "Open Illustrator (the Pulley extension starts with it), wait for it to show as connected, then pull again.",
                        Some(transfer),
                    );
                };
                if let Ok(mut t) = self.transfers.lock() {
                    t.insert(transfer.clone(), Transfer { requester: conn, target, pull: true, created: Instant::now() });
                }
                self.set_transferring();
                info!(transfer, "pull request forwarded to Illustrator");
                self.send(target, &Body::PullRequest { transfer });
            }
            Body::PullError { transfer, code, message, hint } => {
                let t = self.transfers.lock().ok().and_then(|mut t| t.remove(&transfer));
                if let Some(t) = t {
                    if t.target == conn {
                        self.send(t.requester, &Body::PullError { transfer, code, message, hint });
                    }
                }
                self.set_transferring();
            }
            Body::ImportResult { transfer, result } => {
                let t = self.transfers.lock().ok().and_then(|mut t| t.remove(&transfer));
                if let Some(t) = t {
                    // Push: tell the pusher. Pull: the requester is this AE; tell Illustrator.
                    let to = if t.requester == conn { t.target } else { t.requester };
                    info!(transfer, ok = result.ok, layers = result.layers, pull = t.pull, "import finished");
                    self.send(to, &Body::ImportResult { transfer, result });
                }
                self.set_transferring();
            }
            Body::Progress { transfer, done, total, stage } => {
                let other = self.transfers.lock().ok().and_then(|t| t.get(&transfer).map(|t| if t.requester == conn { t.target } else { t.requester }));
                if let Some(o) = other {
                    self.send(o, &Body::Progress { transfer, done, total, stage });
                }
            }
            Body::Cancel { transfer } => {
                let t = self.transfers.lock().ok().and_then(|mut t| t.remove(&transfer));
                if let Some(t) = t {
                    let other = if t.requester == conn { t.target } else { t.requester };
                    self.send(other, &Body::Cancel { transfer });
                }
                self.set_transferring();
            }
            other => {
                debug!(conn, "ignored message type {:?}", std::mem::discriminant(&other));
            }
        }
    }

    async fn process_scene(&self, conn: ConnId, role: Role, id: &str, transfer: String, text: String, pull: bool) {
        if role == Role::Ae {
            return self.send_error(conn, "not_allowed", "After Effects can't send scenes.", "", Some(transfer));
        }
        let bytes = text.len();
        // Validate off the async threads: big scenes take a moment.
        let checked = tokio::task::spawn_blocking(move || parse_scene(&text, Component::Bridge).map(|s| (s, text))).await;
        let (scene, text) = match checked {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                warn!(transfer, code = e.code.as_str(), "rejected scene: {}", e.message);
                self.note_error(&e.message);
                self.send_error(conn, e.code.as_str(), &e.message, &e.hint, Some(transfer.clone()));
                if pull {
                    let t = self.transfers.lock().ok().and_then(|mut t| t.remove(&transfer));
                    if let Some(t) = t {
                        self.send(t.requester, &Body::PullError { transfer, code: e.code.as_str().into(), message: e.message, hint: e.hint });
                    }
                }
                return;
            }
            Err(_) => return self.send_error(conn, "internal", "The bridge failed to check the scene.", "Try again.", Some(transfer)),
        };
        let target = if pull {
            let t = self.transfers.lock().ok().and_then(|t| t.get(&transfer).map(|t| (t.requester, t.target)));
            match t {
                Some((requester, target)) if target == conn => requester,
                _ => return self.send_error(conn, "unknown_transfer", "This pull request is no longer open.", "Pull again from After Effects.", Some(transfer)),
            }
        } else {
            let Some(ae) = self.most_recent(Role::Ae) else {
                return self.send_error(
                    conn,
                    "no_ae",
                    "After Effects is not connected to Pulley.",
                    "Open After Effects (the Pulley extension starts with it), wait until Pulley shows it as connected, then push again.",
                    Some(transfer),
                );
            };
            if let Ok(mut t) = self.transfers.lock() {
                t.insert(transfer.clone(), Transfer { requester: conn, target: ae, pull: false, created: Instant::now() });
            }
            ae
        };
        self.set_transferring();
        let st = pulley_core::validate::validate(&scene).unwrap_or_default();
        info!(transfer, bytes, items = st.items, paths = st.paths, vertices = st.vertices, pull, "scene accepted and forwarded");
        debug!(transfer, id, "forwarding scene");
        let chunks = protocol::chunk(&text);
        let total = chunks.len() as u32;
        if bytes <= CHUNK_BYTES {
            match serde_json::value::RawValue::from_string(text.clone()) {
                Ok(raw) => {
                    self.send(target, &Body::PushScene { transfer: transfer.clone(), scene: raw, pull });
                }
                Err(_) => return self.send_error(conn, "internal", "The bridge failed to forward the scene.", "Try again.", Some(transfer)),
            }
        } else {
            for (i, c) in chunks.iter().enumerate() {
                if !self.send(target, &Body::SceneChunk { transfer: transfer.clone(), index: i as u32, total, data: (*c).to_string(), pull }) {
                    return self.send_error(conn, "send_failed", "After Effects stopped receiving data.", "Check After Effects and try again.", Some(transfer));
                }
            }
        }
        self.send(conn, &Body::Progress { transfer, done: 0, total: 1, stage: "forwarded".into() });
    }
}
