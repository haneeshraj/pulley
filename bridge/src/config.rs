//! Paths, settings and secrets. Everything lives in the user's config directory:
//! `%APPDATA%\Pulley` on Windows, `~/Library/Application Support/Pulley` on macOS.
//! CEP panels find the same folder through `SystemPath.USER_DATA` + `/Pulley`.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const DEFAULT_PORT: u16 = 47290;

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub logs: PathBuf,
    pub tmp: PathBuf,
    pub token: PathBuf,
    pub settings: PathBuf,
}

impl Paths {
    /// The standard per-user location.
    pub fn resolve() -> std::io::Result<Paths> {
        let base = directories::BaseDirs::new().map(|b| b.config_dir().to_path_buf()).ok_or_else(|| std::io::Error::other("no per-user config directory"))?;
        Paths::under(base.join("Pulley"))
    }

    /// A specific root (tests, `--config-dir`).
    pub fn under(root: PathBuf) -> std::io::Result<Paths> {
        let p = Paths { logs: root.join("logs"), tmp: root.join("tmp"), token: root.join("session.token"), settings: root.join("settings.json"), root };
        for d in [&p.root, &p.logs, &p.tmp] {
            fs::create_dir_all(d)?;
            restrict_dir(d);
        }
        Ok(p)
    }
}

#[cfg(unix)]
fn restrict_dir(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir(_p: &Path) {
    // %APPDATA% is already readable only by the user (and administrators/SYSTEM).
}

/// Write a file readable only by the user (0600 on Unix; per-user ACL of %APPDATA% on Windows).
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// Random bytes from the OS, as lowercase hex.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the OS random number generator is unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// A pairing code shown in the tray: 8 characters from an unambiguous alphabet, as XXXX-XXXX.
pub fn pairing_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf).expect("the OS random number generator is unavailable");
    let s: String = buf.iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect();
    format!("{}-{}", &s[..4], &s[4..])
}

/// Normalize a typed pairing code (case, spaces, dashes).
pub fn normalize_code(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FigmaClient {
    pub name: String,
    pub secret: String,
    pub created: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub port: u16,
    pub launch_at_login: bool,
    pub debug_logging: bool,
    pub figma_clients: Vec<FigmaClient>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { port: DEFAULT_PORT, launch_at_login: false, debug_logging: false, figma_clients: Vec::new() }
    }
}

impl Settings {
    /// Load settings; a missing or unreadable file gives defaults (and is logged by the caller).
    pub fn load(paths: &Paths) -> (Settings, Option<String>) {
        match fs::read_to_string(&paths.settings) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(mut s) => {
                    if s.port < 1024 {
                        s.port = DEFAULT_PORT;
                    }
                    s.figma_clients.truncate(64);
                    (s, None)
                }
                Err(e) => (Settings::default(), Some(format!("settings.json is invalid ({e}); using defaults"))),
            },
            Err(_) => (Settings::default(), None),
        }
    }

    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        let text = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        write_private(&paths.settings, &text)
    }
}

/// Create a fresh per-session token and write it where the CEP panels can read it.
#[cfg(test)]
pub fn new_session_token(paths: &Paths) -> std::io::Result<String> {
    let token = random_session_token();
    write_session_token(paths, &token)?;
    Ok(token)
}

/// A fresh session token, not yet written anywhere.
pub fn random_session_token() -> String {
    random_hex(32)
}

/// Publishes the session token for the panels. Only the bridge that owns the port may call this:
/// a second copy that writes it and then fails to bind leaves the running bridge with a token
/// nobody can present.
pub fn write_session_token(paths: &Paths, token: &str) -> std::io::Result<()> {
    write_private(&paths.token, token.as_bytes())
}

/// Seconds since the Unix epoch, as text (for "created" stamps).
pub fn now_stamp() -> String {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pulley-test-{name}-{}", random_hex(4)));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn token_is_written_and_random() {
        let paths = Paths::under(temp_root("token")).unwrap();
        let a = new_session_token(&paths).unwrap();
        let b = new_session_token(&paths).unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(fs::read_to_string(&paths.token).unwrap(), b);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&paths.token).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn settings_roundtrip_and_bad_file() {
        let paths = Paths::under(temp_root("settings")).unwrap();
        let mut s = Settings { debug_logging: true, ..Default::default() };
        s.figma_clients.push(FigmaClient { name: "Figma".into(), secret: "x".into(), created: "1".into() });
        s.save(&paths).unwrap();
        assert_eq!(Settings::load(&paths).0, s);
        fs::write(&paths.settings, "{not json").unwrap();
        let (d, warn) = Settings::load(&paths);
        assert_eq!(d, Settings::default());
        assert!(warn.is_some());
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn pairing_codes() {
        let c = pairing_code();
        assert_eq!(c.len(), 9);
        assert_eq!(&c[4..5], "-");
        assert_eq!(normalize_code(" abcd-efgh "), "ABCDEFGH");
    }
}
