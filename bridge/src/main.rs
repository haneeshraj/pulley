//! Pulley bridge: a small tray (menu bar) app running a WebSocket hub on 127.0.0.1.
//!
//! Options (for development and tests): `--headless` (no tray icon), `--config-dir <dir>`,
//! `--port <n>`, `--stderr` (log to stderr instead of files).
#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod hub;
mod icons;
mod logging;
mod server;
mod tray;

use std::sync::Arc;

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let paths = match arg_value(&args, "--config-dir") {
        Some(d) => config::Paths::under(d.into()),
        None => config::Paths::resolve(),
    };
    let paths = match paths {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Pulley bridge: cannot create its config folder: {e}");
            std::process::exit(1);
        }
    };
    let (mut settings, settings_warning) = config::Settings::load(&paths);
    if let Some(p) = arg_value(&args, "--port").and_then(|p| p.parse::<u16>().ok()) {
        settings.port = p;
    }
    let _log_guard = logging::init(&paths.logs, settings.debug_logging, args.iter().any(|a| a == "--stderr"));
    tracing::info!(version = pulley_core::PULLEY_VERSION, "Pulley bridge starting");
    if let Some(w) = settings_warning {
        tracing::warn!("{w}");
    }
    let token = match config::new_session_token(&paths) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("cannot write the session token: {e}");
            std::process::exit(1);
        }
    };
    let port = settings.port;
    let (hub, summary) = hub::Hub::new(token, paths.clone(), settings);
    let hub = Arc::new(hub);

    // The WebSocket server runs on its own thread; the main thread belongs to the tray's
    // event loop (macOS requires that).
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let server_hub = hub.clone();
    std::thread::Builder::new()
        .name("pulley-server".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            rt.block_on(async move {
                match server::bind(port).await {
                    Ok(listener) => {
                        let _ = ready_tx.send(Ok(()));
                        server::run(listener, server_hub).await;
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                    }
                }
            });
        })
        .expect("cannot start the server thread");

    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            // Usually another Pulley bridge is already running on this port.
            tracing::error!("cannot listen on 127.0.0.1:{port}: {e}");
            std::process::exit(2);
        }
        Err(_) => {
            tracing::error!("the server thread stopped during startup");
            std::process::exit(2);
        }
    }

    // Debug builds only: open a pairing window at start and write the code to a file, so
    // the integration tests can pair a fake Figma client without the tray menu.
    #[cfg(debug_assertions)]
    if args.iter().any(|a| a == "--start-pairing") {
        let code = hub.start_pairing();
        let _ = config::write_private(&paths.root.join("pairing-code.txt"), code.as_bytes());
    }

    if headless {
        tracing::info!("running headless");
        loop {
            std::thread::park();
        }
    }
    tray::run(hub, summary, paths);
}
