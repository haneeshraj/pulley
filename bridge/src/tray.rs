//! Tray (Windows) / menu bar (macOS) icon and menu. Runs on the main thread.

use crate::config::Paths;
use crate::hub::{Hub, Summary};
use crate::icons::{self, TrayState};
use std::sync::Arc;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tokio::sync::watch;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

enum UserEvent {
    Summary(Summary),
    Menu(MenuEvent),
}

fn icon(state: TrayState) -> Option<Icon> {
    let (rgba, w, h) = icons::tray_rgba(state, cfg!(target_os = "macos"));
    Icon::from_rgba(rgba, w, h).ok()
}

fn state_of(s: &Summary) -> TrayState {
    if s.last_error.is_some() {
        TrayState::Error
    } else if s.transferring {
        TrayState::Transferring
    } else {
        TrayState::Idle
    }
}

fn status_text(s: &Summary) -> String {
    let mark = |n: usize| if n > 0 { "connected" } else { "not connected" };
    format!("After Effects: {} · Illustrator: {} · Figma: {}", mark(s.ae), mark(s.illustrator), mark(s.figma))
}

/// Put the pairing code on the clipboard, so it can be pasted into the Figma plugin.
fn copy_to_clipboard(text: &str) -> bool {
    match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string())) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("could not copy the pairing code: {e}");
            false
        }
    }
}

fn code_text(code: &str, copied: bool) -> String {
    if copied { format!("Pairing code: {code} (copied, valid 2 minutes)") } else { format!("Pairing code: {code} (valid 2 minutes)") }
}

fn launcher() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.to_str()?.to_string();
    auto_launch::AutoLaunchBuilder::new().set_app_name("Pulley").set_app_path(&path).build().ok()
}

pub fn run(hub: Arc<Hub>, mut summary: watch::Receiver<Summary>, paths: Paths) {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    {
        let proxy = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e| {
            let _ = proxy.send_event(UserEvent::Menu(e));
        }));
    }
    {
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
            let Ok(rt) = rt else { return };
            rt.block_on(async move {
                while summary.changed().await.is_ok() {
                    let s = summary.borrow().clone();
                    if proxy.send_event(UserEvent::Summary(s)).is_err() {
                        break;
                    }
                }
            });
        });
    }

    let settings = hub.settings();
    let status = MenuItem::new("Pulley: starting", false, None);
    let pair = MenuItem::new("Pair Figma plugin…", true, None);
    let code = MenuItem::new("No pairing code active", false, None);
    let copy = MenuItem::new("Copy pairing code", false, None);
    let mut active_code: Option<String> = None;
    let revoke = MenuItem::new("Forget paired Figma plugins", true, None);
    let login = CheckMenuItem::new("Launch at login", true, settings.launch_at_login, None);
    let debug = CheckMenuItem::new("Debug logging (after restart)", true, settings.debug_logging, None);
    let logs = MenuItem::new(format!("Logs: {}", paths.logs.display()), false, None);
    let quit = MenuItem::new("Quit Pulley", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[
        &status,
        &PredefinedMenuItem::separator(),
        &pair,
        &code,
        &copy,
        &revoke,
        &PredefinedMenuItem::separator(),
        &login,
        &debug,
        &logs,
        &PredefinedMenuItem::separator(),
        &quit,
    ]);
    let mut tray: Option<TrayIcon> = None;
    let mut current = TrayState::Idle;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                // macOS: the icon must be created once the event loop is running.
                let mut b = TrayIconBuilder::new().with_menu(Box::new(menu.clone())).with_tooltip("Pulley");
                if let Some(i) = icon(TrayState::Idle) {
                    // macOS draws the menu bar icon as a template, so it follows light and dark menu bars.
                    #[cfg(target_os = "macos")]
                    {
                        b = b.with_icon_templated(i);
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        b = b.with_icon(i);
                    }
                }
                tray = b.build().ok();
                status.set_text(status_text(&Summary::default()));
            }
            Event::UserEvent(UserEvent::Summary(s)) => {
                status.set_text(status_text(&s));
                match &s.pairing_code {
                    Some(c) => {
                        if active_code.as_deref() != Some(c.as_str()) {
                            code.set_text(code_text(c, false));
                        }
                        active_code = Some(c.clone());
                        copy.set_enabled(true);
                    }
                    None => {
                        code.set_text("No pairing code active");
                        active_code = None;
                        copy.set_enabled(false);
                    }
                }
                let st = state_of(&s);
                if st != current {
                    current = st;
                    if let Some(t) = &tray {
                        // set_icon would draw it as a plain image on macOS; keep it a template there.
                        #[cfg(target_os = "macos")]
                        let _ = t.set_icon_templated(icon(st));
                        #[cfg(not(target_os = "macos"))]
                        let _ = t.set_icon(icon(st));
                    }
                }
                if let Some(t) = &tray {
                    let tip = match st {
                        TrayState::Transferring => "Pulley: transferring".to_string(),
                        TrayState::Error => format!("Pulley: {}", s.last_error.clone().unwrap_or_default()),
                        TrayState::Idle => "Pulley".to_string(),
                    };
                    let _ = t.set_tooltip(Some(tip.chars().take(120).collect::<String>()));
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                if e.id == pair.id() {
                    // A new code, copied right away: paste it into the plugin.
                    let c = hub.start_pairing();
                    code.set_text(code_text(&c, copy_to_clipboard(&c)));
                    copy.set_enabled(true);
                    active_code = Some(c);
                } else if e.id == copy.id() {
                    if let Some(c) = &active_code {
                        code.set_text(code_text(c, copy_to_clipboard(c)));
                    }
                } else if e.id == revoke.id() {
                    hub.revoke_figma();
                } else if e.id == login.id() {
                    let on = login.is_checked();
                    if let Some(l) = launcher() {
                        let r = if on { l.enable() } else { l.disable() };
                        if let Err(err) = r {
                            tracing::warn!("launch at login: {err}");
                            login.set_checked(!on);
                        } else {
                            hub.update_settings(|s| s.launch_at_login = on);
                        }
                    }
                } else if e.id == debug.id() {
                    let on = debug.is_checked();
                    hub.update_settings(|s| s.debug_logging = on);
                } else if e.id == quit.id() {
                    tracing::info!("quit from the tray menu");
                    tray = None;
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}
