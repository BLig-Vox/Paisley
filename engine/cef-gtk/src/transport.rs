/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::key_tables::KeyLocation;
use async_channel;
use gio::prelude::*;
use gio::{Subprocess, SubprocessFlags, SubprocessLauncher};
use glib::{debug, error, info, warn};
use paisley_engine_transport::{MAX_ACTION_BYTES, action_channel, packet_length};
use prost::Message;
use std::cell::Cell;
use std::ffi::OsStr;
use std::rc::Rc;

use crate::proto_ipc::{ServoAction, ServoEvent, servo_action};

const G_LOG_DOMAIN: &str = "ServoGtk";

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl From<i32> for LogLevel {
    fn from(value: i32) -> Self {
        match value {
            0 => LogLevel::Debug,
            1 => LogLevel::Info,
            2 => LogLevel::Warn,
            3 => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }
}

struct EngineProcess {
    commands: async_channel::Sender<Vec<u8>>,
    routes: std::cell::RefCell<
        std::collections::HashMap<u64, Rc<paisley_engine_transport::mailbox::Mailbox<ServoEvent>>>,
    >,
    subprocess: Subprocess,
    closing: Cell<bool>,
    next_id: Cell<u64>,
    queued_bytes: Rc<Cell<usize>>,
}
thread_local! {
    static ENGINES: std::cell::RefCell<std::collections::HashMap<(String, bool), std::rc::Weak<EngineProcess>>> = Default::default();
    static STOPPING: std::cell::RefCell<Vec<(Subprocess, Rc<Cell<bool>>)>> = Default::default();
}

impl EngineProcess {
    fn acquire(config_dir: Option<&str>, temporary: bool) -> Rc<Self> {
        let key = (config_dir.unwrap_or_default().to_owned(), temporary);
        ENGINES.with(|registry| {
            if let Some(engine) = registry.borrow().get(&key).and_then(std::rc::Weak::upgrade)
                && !engine.closing.get()
            {
                return engine;
            }
            let engine = Self::spawn(config_dir, temporary);
            let mut registry = registry.borrow_mut();
            registry.retain(|_, value| value.strong_count() > 0);
            registry.insert(key, Rc::downgrade(&engine));
            engine
        })
    }

    fn spawn(config_dir: Option<&str>, temporary: bool) -> Rc<Self> {
        let launcher =
            SubprocessLauncher::new(SubprocessFlags::STDIN_PIPE | SubprocessFlags::STDOUT_PIPE);
        let exe = std::env::current_exe().expect("Could not locate Servo runner executable");
        let mut args = vec![exe.as_os_str(), OsStr::new(crate::runner::RUNNER_ARG)];
        let parent_pid = std::process::id().to_string();
        args.extend([OsStr::new("--servo-parent-pid"), OsStr::new(&parent_pid)]);
        if let Some(dir) = config_dir {
            args.extend([OsStr::new("--servo-config-dir"), OsStr::new(dir)]);
        }
        if temporary {
            args.push(OsStr::new("--servo-temporary-storage"));
        }
        let subprocess = launcher.spawn(&args).expect("Could not start Servo runner");
        let stdin = subprocess.stdin_pipe().unwrap();
        let stdout = subprocess.stdout_pipe().unwrap();
        let (commands, pending) = action_channel::<Vec<u8>>();
        let queued_bytes = Rc::new(Cell::new(0usize));
        let process = Rc::new(Self {
            commands,
            routes: Default::default(),
            subprocess,
            closing: Cell::new(false),
            next_id: Cell::new(1),
            queued_bytes: queued_bytes.clone(),
        });
        let weak = Rc::downgrade(&process);
        glib::spawn_future_local(async move {
            while let Ok(frame) = pending.recv().await {
                let size = frame.len();
                let result = stdin.write_all_future(frame, glib::Priority::DEFAULT).await;
                queued_bytes.set(queued_bytes.get().saturating_sub(size));
                if result.is_err() {
                    break;
                }
            }
            if let Some(process) = weak.upgrade() {
                process.shutdown();
            }
        });
        let weak = Rc::downgrade(&process);
        glib::spawn_future_local(async move {
            loop {
                let header = match stdout
                    .read_all_future(vec![0u8; 4], glib::Priority::DEFAULT)
                    .await
                {
                    Ok((data, 4, None)) => data,
                    _ => break,
                };
                let Ok(len) = packet_length(header.try_into().unwrap()) else {
                    break;
                };
                let body = match stdout
                    .read_all_future(vec![0u8; len], glib::Priority::DEFAULT)
                    .await
                {
                    Ok((data, count, None)) if count == len => data,
                    _ => break,
                };
                let Ok(event) = ServoEvent::decode_from_slice(&body) else {
                    break;
                };
                let Some(process) = weak.upgrade() else {
                    break;
                };
                let inbox = process.routes.borrow().get(&event.webview_id).cloned();
                if let Some(inbox) = inbox {
                    match event.event.as_ref() {
                        Some(crate::proto_ipc::servo_event::Event::FrameReady(_)) => {
                            inbox.push_frame(event)
                        }
                        Some(crate::proto_ipc::servo_event::Event::PopupFrame(_)) => {
                            inbox.push_aux_frame(event)
                        }
                        _ => {
                            if inbox.push_control(event, len).is_err() {
                                process.shutdown();
                                break;
                            }
                        }
                    }
                }
            }
            if let Some(process) = weak.upgrade() {
                process.shutdown();
            }
        });
        process
    }

    fn send(&self, id: u64, mut action: ServoAction) {
        if self.closing.get() {
            return;
        }
        action.webview_id = id;
        let len = action.encoded_len();
        if len > MAX_ACTION_BYTES
            || len + 4 > (2 * 1024 * 1024usize).saturating_sub(self.queued_bytes.get())
        {
            error!("Servo IPC command budget exceeded");
            self.shutdown();
            return;
        }
        let mut framed = Vec::with_capacity(4 + len);
        framed.extend_from_slice(&(len as u32).to_le_bytes());
        action.encode(&mut framed).unwrap();
        self.queued_bytes
            .set(self.queued_bytes.get() + framed.len());
        if let Err(error) = self.commands.try_send(framed) {
            self.queued_bytes.set(
                self.queued_bytes
                    .get()
                    .saturating_sub(error.into_inner().len()),
            );
            self.shutdown();
        }
    }

    fn shutdown(&self) {
        if self.closing.replace(true) {
            return;
        }
        for (_, inbox) in self.routes.borrow_mut().drain() {
            inbox.close();
        }
        let message = ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Shutdown(true)),
        };
        let mut bytes = Vec::with_capacity(message.encoded_len() + 4);
        bytes.extend_from_slice(&(message.encoded_len() as u32).to_le_bytes());
        message.encode(&mut bytes).unwrap();
        if self.commands.try_send(bytes).is_err() {
            self.subprocess.force_exit();
        }
        self.commands.close();
        let done = Rc::new(Cell::new(false));
        STOPPING.with(|pending| {
            let mut pending = pending.borrow_mut();
            pending.retain(|(_, done)| !done.get());
            pending.push((self.subprocess.clone(), done.clone()));
        });
        let completed = done.clone();
        self.subprocess
            .wait_async(None::<&gio::Cancellable>, move |_| completed.set(true));
        let subprocess = self.subprocess.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(5), move || {
            if !done.get() {
                subprocess.force_exit();
            }
        });
    }
}
impl Drop for EngineProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct PageRunner {
    engine: Rc<EngineProcess>,
    id: u64,
    event_receiver: Rc<paisley_engine_transport::mailbox::Mailbox<ServoEvent>>,
    closing: Cell<bool>,
}
impl PageRunner {
    pub fn new(config_dir: Option<&str>, temporary: bool) -> Self {
        Self::with_target(config_dir, temporary, 0)
    }
    pub fn inspector(config_dir: Option<&str>, temporary: bool, target: u64) -> Self {
        Self::with_target(config_dir, temporary, target)
    }
    fn with_target(config_dir: Option<&str>, temporary: bool, target: u64) -> Self {
        let engine = EngineProcess::acquire(config_dir, temporary);
        let id = engine.next_id.get();
        engine
            .next_id
            .set(id.checked_add(1).expect("Servo tab ID exhausted"));
        let inbox = Rc::new(paisley_engine_transport::mailbox::Mailbox::new());
        engine.routes.borrow_mut().insert(id, inbox.clone());
        engine.send(
            id,
            ServoAction {
                webview_id: 0,
                action: Some(if target == 0 {
                    servo_action::Action::CreateView(true)
                } else {
                    servo_action::Action::InspectPage(target)
                }),
            },
        );
        Self {
            engine,
            id,
            event_receiver: inbox,
            closing: Cell::new(false),
        }
    }
    fn send_action(&self, action: ServoAction) {
        if !self.closing.get() {
            self.engine.send(self.id, action);
        }
    }
    pub fn event_receiver(&self) -> Rc<paisley_engine_transport::mailbox::Mailbox<ServoEvent>> {
        self.event_receiver.clone()
    }
    pub fn is_closed(&self) -> bool {
        self.closing.get() || self.engine.closing.get()
    }
    pub fn page_id(&self) -> u64 {
        self.id
    }
    pub fn identifier(&self) -> Option<String> {
        self.engine.subprocess.identifier().map(|s| s.to_string())
    }
    pub fn set_blocking(&self, enabled: bool, exceptions: Vec<String>) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Blocking(crate::proto_ipc::Blocking {
                enabled,
                exceptions,
            })),
        });
    }
    pub fn set_visible(&self, visible: bool) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Visible(visible)),
        });
    }

    pub fn set_focused(&self, focused: bool) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Focused(focused)),
        });
    }

    pub fn load_url(&self, url: &str) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::LoadUrl(crate::proto_ipc::LoadUrl {
                url: url.to_string(),
            })),
        });
    }

    pub fn load_html(&self, html: &str, base_url: Option<&str>) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::LoadHtml(crate::proto_ipc::LoadHtml {
                html: html.to_string(),
                base_url: base_url.unwrap_or_default().to_string(),
            })),
        });
    }

    pub fn stop_loading(&self) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::StopLoading(true)),
        });
    }

    pub fn reload(&self) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Reload(true)),
        });
    }

    pub fn go_back(&self) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::GoBack(true)),
        });
    }

    pub fn go_forward(&self) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::GoForward(true)),
        });
    }

    pub fn resize(&self, width: u32, height: u32) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Resize(crate::proto_ipc::Resize {
                width,
                height,
            })),
        });
    }

    pub fn motion(&self, x: f64, y: f64) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Motion(crate::proto_ipc::Motion {
                x,
                y,
            })),
        });
    }

    pub fn button_press(&self, button: u32, x: f64, y: f64, click_count: i32) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::ButtonPress(
                crate::proto_ipc::ButtonPress {
                    button,
                    x,
                    y,
                    click_count,
                },
            )),
        });
    }

    pub fn button_release(&self, button: u32, x: f64, y: f64, click_count: i32) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::ButtonRelease(
                crate::proto_ipc::ButtonRelease {
                    button,
                    x,
                    y,
                    click_count,
                },
            )),
        });
    }

    fn convert_location(location: KeyLocation) -> crate::proto_ipc::Location {
        match location {
            KeyLocation::Standard => crate::proto_ipc::Location::Standard,
            KeyLocation::Left => crate::proto_ipc::Location::Left,
            KeyLocation::Right => crate::proto_ipc::Location::Right,
            KeyLocation::Numpad => crate::proto_ipc::Location::Numpad,
        }
    }

    pub fn key_press(
        &self,
        key: String,
        is_character: bool,
        location: KeyLocation,
        key_code: u32,
        modifiers: u32,
    ) {
        let key_type = if is_character {
            crate::proto_ipc::KeyType::Character
        } else {
            crate::proto_ipc::KeyType::Named
        };
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::KeyPress(crate::proto_ipc::KeyPress {
                key,
                key_type: key_type as i32,
                location: Self::convert_location(location) as i32,
                key_code,
                modifiers,
            })),
        });
    }

    pub fn key_release(
        &self,
        key: String,
        is_character: bool,
        location: KeyLocation,
        key_code: u32,
        modifiers: u32,
    ) {
        let key_type = if is_character {
            crate::proto_ipc::KeyType::Character
        } else {
            crate::proto_ipc::KeyType::Named
        };
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::KeyRelease(
                crate::proto_ipc::KeyRelease {
                    key,
                    key_type: key_type as i32,
                    location: Self::convert_location(location) as i32,
                    key_code,
                    modifiers,
                },
            )),
        });
    }

    pub fn scroll(&self, dx: f64, dy: f64) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Scroll(crate::proto_ipc::Scroll {
                dx,
                dy,
            })),
        });
    }

    pub fn touch_begin(&self, x: f64, y: f64) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::TouchBegin(
                crate::proto_ipc::TouchBegin { x, y },
            )),
        });
    }

    pub fn touch_update(&self, x: f64, y: f64) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::TouchUpdate(
                crate::proto_ipc::TouchUpdate { x, y },
            )),
        });
    }

    pub fn touch_end(&self, x: f64, y: f64) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::TouchEnd(crate::proto_ipc::TouchEnd {
                x,
                y,
            })),
        });
    }

    pub fn touch_cancel(&self, x: f64, y: f64) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::TouchCancel(
                crate::proto_ipc::TouchCancel { x, y },
            )),
        });
    }

    pub fn shutdown(&self) {
        if self.closing.replace(true) {
            return;
        }
        self.engine.send(
            self.id,
            ServoAction {
                webview_id: 0,
                action: Some(servo_action::Action::CloseView(true)),
            },
        );
        if let Some(inbox) = self.engine.routes.borrow_mut().remove(&self.id) {
            inbox.close();
        }
        let empty = self.engine.routes.borrow().is_empty();
        if empty {
            self.engine.shutdown();
        }
    }

    pub fn add_user_script(&self, source: &str) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::AddUserScript(
                crate::proto_ipc::AddUserScript {
                    source: source.to_string(),
                },
            )),
        });
    }

    pub fn add_user_style_sheet(&self, source: &str) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::AddUserStyleSheet(
                crate::proto_ipc::AddUserStyleSheet {
                    source: source.to_string(),
                },
            )),
        });
    }

    pub fn remove_all_user_scripts(&self) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::RemoveAllUserScripts(true)),
        });
    }

    pub fn remove_all_user_style_sheets(&self) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::RemoveAllUserStyleSheets(true)),
        });
    }

    pub fn register_script_message_handler(&self, name: &str) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::RegisterScriptMessageHandler(
                crate::proto_ipc::RegisterScriptMessageHandler {
                    name: name.to_string(),
                },
            )),
        });
    }

    pub fn unregister_script_message_handler(&self, name: &str) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::UnregisterScriptMessageHandler(
                crate::proto_ipc::UnregisterScriptMessageHandler {
                    name: name.to_string(),
                },
            )),
        });
    }

    pub fn evaluate_javascript(&self, source: &str) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::EvaluateJavascript(
                crate::proto_ipc::EvaluateJavascript {
                    source: source.to_string(),
                },
            )),
        });
    }

    pub fn set_zoom_level(&self, level: f64) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::SetZoomLevel(
                crate::proto_ipc::SetZoomLevel { level },
            )),
        });
    }

    pub fn set_hidpi_scale_factor(&self, scale: f32) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::SetHidpiScaleFactor(
                crate::proto_ipc::SetHidpiScaleFactor { scale },
            )),
        });
    }

    /// Start a request/response round trip that exercises the bidirectional
    /// IPC channel end to end. The runner replies with a `PingRequest` event,
    /// which the [`WebView`](crate::WebView) answers via
    /// [`send_ping_response`](Self::send_ping_response).
    pub fn ping(&self) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Ping(crate::proto_ipc::Ping {})),
        });
    }

    /// Answer a runner request identified by `request_id` with a ping payload.
    ///
    /// This is the return path of the bidirectional channel: the runner
    /// correlates the response to the pending request it stashed under the same
    /// id. Real delegate features add their own typed `send_*_response`
    /// helpers alongside this one.
    pub fn send_ping_response(&self, request_id: u64, ok: bool) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Response(
                crate::proto_ipc::ServoResponse {
                    request_id,
                    payload: Some(crate::proto_ipc::servo_response::Payload::Ping(
                        crate::proto_ipc::PingResponse { ok },
                    )),
                },
            )),
        });
    }

    /// Answer a script dialog request (alert/confirm/prompt) identified by
    /// `request_id`. `confirmed` is whether the user accepted; `value` is the
    /// prompt text (ignored for alert/confirm).
    pub fn send_script_dialog_response(&self, request_id: u64, confirmed: bool, value: &str) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Response(
                crate::proto_ipc::ServoResponse {
                    request_id,
                    payload: Some(crate::proto_ipc::servo_response::Payload::ScriptDialog(
                        crate::proto_ipc::ScriptDialogResponse {
                            confirmed,
                            value: value.to_string(),
                        },
                    )),
                },
            )),
        });
    }

    /// Answer a file chooser request identified by `request_id` with the
    /// selected `paths`. An empty list means the user cancelled.
    pub fn send_file_chooser_response(&self, request_id: u64, paths: Vec<String>) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Response(
                crate::proto_ipc::ServoResponse {
                    request_id,
                    payload: Some(crate::proto_ipc::servo_response::Payload::FileChooser(
                        crate::proto_ipc::FileChooserResponse { paths },
                    )),
                },
            )),
        });
    }

    /// Answer an HTTP authentication request identified by `request_id`. When
    /// `confirmed` is false the challenge is cancelled and the credentials are
    /// ignored.
    pub fn send_auth_response(
        &self,
        request_id: u64,
        confirmed: bool,
        username: &str,
        password: &str,
    ) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Response(
                crate::proto_ipc::ServoResponse {
                    request_id,
                    payload: Some(crate::proto_ipc::servo_response::Payload::Auth(
                        crate::proto_ipc::AuthResponse {
                            confirmed,
                            username: username.to_string(),
                            password: password.to_string(),
                        },
                    )),
                },
            )),
        });
    }

    /// Answer a permission request identified by `request_id`.
    pub fn send_permission_response(&self, request_id: u64, allow: bool) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Response(
                crate::proto_ipc::ServoResponse {
                    request_id,
                    payload: Some(crate::proto_ipc::servo_response::Payload::Permission(
                        crate::proto_ipc::PermissionResponse { allow },
                    )),
                },
            )),
        });
    }

    /// Answer a context menu request identified by `request_id`. When
    /// `selected` is true, `index` is the chosen entry's index; otherwise the
    /// menu was dismissed.
    pub fn send_context_menu_response(&self, request_id: u64, selected: bool, index: u32) {
        self.send_action(ServoAction {
            webview_id: 0,
            action: Some(servo_action::Action::Response(
                crate::proto_ipc::ServoResponse {
                    request_id,
                    payload: Some(crate::proto_ipc::servo_response::Payload::ContextMenu(
                        crate::proto_ipc::ContextMenuResponse { selected, index },
                    )),
                },
            )),
        });
    }

    pub fn handle_log_message(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Debug => {
                debug!("{}", message);
            }
            LogLevel::Info => {
                info!("{}", message);
            }
            LogLevel::Warn => {
                warn!("{}", message);
            }
            LogLevel::Error => {
                error!("{}", message);
            }
        }
    }
}

impl Drop for PageRunner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Give profile storage a bounded graceful shutdown after GTK's last window
/// closes, then stop any nonresponsive owned renderer before the parent exits.
pub fn finish_shutdown() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(7);
    loop {
        let finished = STOPPING.with(|pending| pending.borrow().iter().all(|(_, done)| done.get()));
        if finished || std::time::Instant::now() >= deadline {
            break;
        }
        while glib::MainContext::default().pending() && std::time::Instant::now() < deadline {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    STOPPING.with(|pending| {
        for (process, done) in pending.borrow_mut().drain(..) {
            if !done.get() {
                process.force_exit();
            }
        }
    });
}

/// Profile-owned control route. Keeps downloads/storage alive independently of tabs.
pub struct ProfileSession {
    engine: Rc<EngineProcess>,
    inbox: Rc<paisley_engine_transport::mailbox::Mailbox<ServoEvent>>,
}
impl ProfileSession {
    pub fn new(config_dir: &str, temporary: bool, callback: impl Fn(ServoEvent) + 'static) -> Self {
        let engine = EngineProcess::acquire(Some(config_dir), temporary);
        assert!(
            !engine.routes.borrow().contains_key(&0),
            "one service owner per profile"
        );
        let inbox = Rc::new(paisley_engine_transport::mailbox::Mailbox::new());
        engine.routes.borrow_mut().insert(0, inbox.clone());
        let events = inbox.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = events.recv().await {
                callback(event);
            }
            callback(ServoEvent {
                webview_id: 0,
                event: None,
            });
        });
        Self { engine, inbox }
    }
    pub fn send(&self, action: servo_action::Action) {
        self.engine.send(
            0,
            ServoAction {
                webview_id: 0,
                action: Some(action),
            },
        );
    }
    pub fn is_closed(&self) -> bool {
        self.engine.closing.get()
    }
    #[cfg(feature = "test-support")]
    pub fn terminate_for_test(&self) {
        self.engine.subprocess.force_exit();
    }
    pub fn query_stats(&self) {
        self.send(servo_action::Action::QueryStats(true));
    }
}
impl Drop for ProfileSession {
    fn drop(&mut self) {
        self.engine.routes.borrow_mut().remove(&0);
        self.inbox.close();
        if self.engine.routes.borrow().is_empty() {
            self.engine.shutdown();
        }
    }
}
