//! Dedicated ephemeral WebKit automation UI. Never opens a normal Profile.
//! Only the isolated MCP worker starts this mode; control is authenticated and
//! restricted to status, trusted action display, and revocation (not execution).
use gtk::{glib, prelude::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixListener,
    },
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use webkit::prelude::*;

enum Control {
    Request(Value, mpsc::SyncSender<Value>),
    Closed,
}

#[derive(Clone, Copy)]
enum StopReason {
    StopButton,
    NativeKey,
    NativePointer,
    WindowClosed,
    MissingSession,
    NonEphemeralSession,
    GrantDenied,
    ActionStop,
    ProtocolStop,
    GrantExpired,
    ControlClosed,
}

impl StopReason {
    fn label(self) -> &'static str {
        match self {
            Self::StopButton => "stop-button",
            Self::NativeKey => "native-key-preemption",
            Self::NativePointer => "native-pointer-preemption",
            Self::WindowClosed => "window-closed",
            Self::MissingSession => "automation-session-unavailable",
            Self::NonEphemeralSession => "non-ephemeral-session-refused",
            Self::GrantDenied => "workspace-grant-denied",
            Self::ActionStop => "action-consent-stop",
            Self::ProtocolStop => "protocol-stop",
            Self::GrantExpired => "workspace-grant-expired",
            Self::ControlClosed => "control-owner-disconnected",
        }
    }
}

struct Prompt {
    window: gtk::Window,
    finish: Rc<dyn Fn(gtk::ResponseType)>,
}

struct Runtime {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    overlay: gtk::Overlay,
    label: gtk::Label,
    marker: gtk::DrawingArea,
    point: Rc<Cell<Option<(f64, f64)>>>,
    context: webkit::WebContext,
    manager: webkit::UserContentManager,
    view: RefCell<Option<webkit::WebView>>,
    generation: Cell<u64>,
    stopped: Cell<bool>,
    origin: String,
    proxy: String,
    fixture: bool,
    granted_until: Cell<Option<Instant>>,
    filter: RefCell<Option<webkit::UserContentFilter>>,
    filter_ready: Cell<bool>,
    filter_pending: Cell<bool>,
    filter_cancel: gtk::gio::Cancellable,
    filter_store: RefCell<Option<webkit::UserContentFilterStore>>,
    filter_dir: RefCell<Option<PathBuf>>,
    prompts: RefCell<Vec<Prompt>>,
}

impl Runtime {
    fn build(app: &gtk::Application, origin: String, proxy: String, fixture: bool) -> Rc<Self> {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Paisley — AUTOMATION · ephemeral origin workspace")
            .default_width(1280)
            .default_height(800)
            .build();
        let label = gtk::Label::builder()
            .label(format!("AUTOMATION · ephemeral · {origin} · Waiting"))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let stop = gtk::Button::with_label("Stop automation (Esc)");
        stop.add_css_class("destructive-action");
        let chrome = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        chrome.set_margin_start(12);
        chrome.set_margin_end(12);
        chrome.set_margin_top(8);
        chrome.set_margin_bottom(8);
        chrome.append(&label);
        chrome.append(&stop);
        let overlay = gtk::Overlay::builder().vexpand(true).hexpand(true).build();
        let marker = gtk::DrawingArea::builder()
            .can_target(false)
            .hexpand(true)
            .vexpand(true)
            .build();
        let point = Rc::new(Cell::new(None::<(f64, f64)>));
        let drawing = Rc::clone(&point);
        marker.set_draw_func(move |_, cr, _, _| {
            if let Some((x, y)) = drawing.get() {
                cr.set_source_rgba(0.95, 0.2, 0.1, 0.9);
                cr.set_line_width(3.0);
                cr.arc(x, y, 12.0, 0.0, std::f64::consts::TAU);
                let _ = cr.stroke();
                cr.move_to(x - 18.0, y);
                cr.line_to(x + 18.0, y);
                cr.move_to(x, y - 18.0);
                cr.line_to(x, y + 18.0);
                let _ = cr.stroke();
            }
        });
        overlay.add_overlay(&marker);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&chrome);
        root.append(&overlay);
        window.set_child(Some(&root));
        let context = webkit::WebContext::new();
        context.set_automation_allowed(true);
        let this = Rc::new(Self {
            app: app.clone(),
            window,
            overlay,
            label,
            marker,
            point,
            context,
            manager: webkit::UserContentManager::new(),
            view: RefCell::new(None),
            generation: Cell::new(0),
            stopped: Cell::new(false),
            origin,
            proxy,
            fixture,
            granted_until: Cell::new(fixture.then(|| Instant::now() + Duration::from_secs(600))),
            filter: RefCell::new(None),
            filter_ready: Cell::new(false),
            filter_pending: Cell::new(false),
            filter_cancel: gtk::gio::Cancellable::new(),
            filter_store: RefCell::new(None),
            filter_dir: RefCell::new(None),
            prompts: RefCell::new(Vec::new()),
        });
        this.compile_filter();
        let weak = Rc::downgrade(&this);
        stop.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.stop(StopReason::StopButton);
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&this);
        keys.connect_key_pressed(move |_, _, _, _| {
            // WebDriver delivers input within WebKit, not as desktop GTK events.
            // Any actual native keyboard event preempts automation.
            if let Some(this) = weak.upgrade()
                && this.granted()
            {
                this.stop(StopReason::NativeKey);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        this.window.add_controller(keys);
        let gesture = gtk::GestureClick::new();
        gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&this);
        gesture.connect_pressed(move |_, _, _, _| {
            if let Some(this) = weak.upgrade() {
                this.stop(StopReason::NativePointer);
            }
        });
        this.overlay.add_controller(gesture);
        let weak = Rc::downgrade(&this);
        this.window.connect_close_request(move |_| {
            if let Some(this) = weak.upgrade() {
                this.stop(StopReason::WindowClosed);
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&this);
        this.context
            .connect_automation_started(move |context, session| {
                let Some(this) = weak.upgrade() else {
                    return;
                };
                let info = webkit::ApplicationInfo::new();
                info.set_name("Paisley");
                info.set_version(0, 1, 0);
                session.set_application_info(&info);
                let Some(network) = context.network_session_for_automation() else {
                    this.stop(StopReason::MissingSession);
                    return;
                };
                if !network.is_ephemeral() {
                    this.stop(StopReason::NonEphemeralSession);
                    return;
                }
                network.set_proxy_settings(
                    webkit::NetworkProxyMode::Custom,
                    Some(&webkit::NetworkProxySettings::new(Some(&this.proxy), &[])),
                );
                network.connect_download_started(|_, download| download.cancel());
                let view = webkit::WebView::builder()
                    .user_content_manager(&this.manager)
                    .web_context(context)
                    .network_session(&network)
                    .is_controlled_by_automation(true)
                    .hexpand(true)
                    .vexpand(true)
                    .build();
                if let Some(settings) = webkit::prelude::WebViewExt::settings(&view) {
                    settings.set_hardware_acceleration_policy(
                        webkit::HardwareAccelerationPolicy::Never,
                    );
                }
                // The adapter exposes no create-window command. WebKit's typed
                // create-web-view signal requires a non-null result; always return
                // this one approved view, never allocate a second context.
                let approved = view.clone();
                session.connect_create_web_view(None, move |_| approved.clone());
                view.connect_create(|_, _| None);
                view.connect_permission_request(|_, request| {
                    request.deny();
                    true
                });
                view.connect_run_file_chooser(|_, request| {
                    request.cancel();
                    true
                });
                let weak = Rc::downgrade(&this);
                view.connect_decide_policy(move |_, decision, kind| {
                    if matches!(
                        kind,
                        webkit::PolicyDecisionType::NavigationAction
                            | webkit::PolicyDecisionType::NewWindowAction
                    ) {
                        let uri = decision
                            .downcast_ref::<webkit::NavigationPolicyDecision>()
                            .and_then(|nav| nav.navigation_action())
                            .and_then(|action| action.request())
                            .and_then(|request| request.uri());
                        let permitted = kind != webkit::PolicyDecisionType::NewWindowAction
                            && uri.as_ref().is_some_and(|uri| {
                                uri.as_str() == "about:blank"
                                    || weak.upgrade().is_some_and(|this| {
                                        this.granted()
                                            && this.filter_ready.get()
                                            && allowed(uri, &this.origin)
                                    })
                            });
                        if !permitted {
                            decision.ignore();
                            return true;
                        }
                    }
                    if let Some(response) =
                        decision.downcast_ref::<webkit::ResponsePolicyDecision>()
                        && !response.is_mime_type_supported()
                    {
                        decision.ignore();
                        return true;
                    }
                    false
                });
                let weak = Rc::downgrade(&this);
                view.connect_load_changed(move |_, event| {
                    if event == webkit::LoadEvent::Started
                        && let Some(this) = weak.upgrade()
                    {
                        this.generation.set(this.generation.get().wrapping_add(1));
                    }
                });
                this.overlay.set_child(Some(&view));
                *this.view.borrow_mut() = Some(view);
            });
        this.window.present();
        if !fixture {
            this.workspace_prompt();
        }
        this
    }

    fn granted(&self) -> bool {
        !self.stopped.get() && grant_valid(self.granted_until.get(), Instant::now())
    }

    fn compile_filter(self: &Rc<Self>) {
        let stamp = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(value) => value.as_nanos(),
            Err(_) => return,
        };
        let path = PathBuf::from(format!(
            "/tmp/paisley-automation-filter-{}-{stamp}",
            std::process::id()
        ));
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        if builder.create(&path).is_err() {
            return;
        }
        let Some(path_str) = path.to_str() else {
            return;
        };
        let store = webkit::UserContentFilterStore::new(path_str);
        let escaped: String = self
            .origin
            .chars()
            .flat_map(|c| {
                if matches!(
                    c,
                    '.' | '+'
                        | '*'
                        | '?'
                        | '^'
                        | '$'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '|'
                        | '\\'
                ) {
                    vec!['\\', c]
                } else {
                    vec![c]
                }
            })
            .collect();
        let rules = json!([
            {"trigger":{"url-filter":"^[a-zA-Z][a-zA-Z0-9+.-]*://"},"action":{"type":"block"}},
            {"trigger":{"url-filter":format!("^{escaped}/")},"action":{"type":"ignore-previous-rules"}}
        ]).to_string();
        *self.filter_dir.borrow_mut() = Some(path);
        *self.filter_store.borrow_mut() = Some(store.clone());
        self.filter_pending.set(true);
        // Retain the store/directory until completion, including cancellation.
        let this = Rc::clone(self);
        store.save(
            "origin-allowlist",
            &glib::Bytes::from_owned(rules.into_bytes()),
            Some(&self.filter_cancel),
            move |result| {
                this.filter_pending.set(false);
                if this.stopped.get() {
                    return;
                }
                if let Ok(filter) = result {
                    this.manager.add_filter(&filter);
                    *this.filter.borrow_mut() = Some(filter);
                    this.filter_ready.set(true);
                }
            },
        );
    }

    fn prompt(
        self: &Rc<Self>,
        title: &str,
        message: &str,
        choices: &[(&str, gtk::ResponseType)],
        response: impl FnOnce(gtk::ResponseType) + 'static,
    ) {
        // A separate native window: WebDriver/page content cannot operate these
        // controls. Use current GTK widgets, not the deprecated GtkDialog API.
        let window = gtk::Window::builder()
            .transient_for(&self.window)
            .modal(true)
            .title(title)
            .default_width(560)
            .resizable(false)
            .build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 16);
        root.set_margin_top(16);
        root.set_margin_bottom(16);
        root.set_margin_start(16);
        root.set_margin_end(16);
        let text = gtk::Label::new(Some(message));
        text.set_wrap(true);
        text.set_max_width_chars(70);
        text.set_xalign(0.0);
        root.append(&text);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        root.append(&actions);
        window.set_child(Some(&root));
        let weak = Rc::downgrade(self);
        let weak_window = window.downgrade();
        let response = RefCell::new(Some(response));
        let finish: Rc<dyn Fn(gtk::ResponseType)> = Rc::new(move |value| {
            let callback = response.borrow_mut().take();
            if let Some(callback) = callback {
                if let Some(this) = weak.upgrade() {
                    this.prompts
                        .borrow_mut()
                        .retain(|item| Some(item.window.clone()) != weak_window.upgrade());
                }
                callback(value);
                if let Some(window) = weak_window.upgrade() {
                    window.close();
                }
            }
        });
        let mut cancel = None;
        for (label, value) in choices {
            let button = gtk::Button::with_label(label);
            let finish = Rc::clone(&finish);
            let value = *value;
            button.connect_clicked(move |_| finish(value));
            if value == gtk::ResponseType::Cancel {
                window.set_default_widget(Some(&button));
                cancel = Some(button.clone());
            }
            actions.append(&button);
        }
        let close = Rc::clone(&finish);
        window.connect_close_request(move |_| {
            close(gtk::ResponseType::Cancel);
            glib::Propagation::Proceed
        });
        self.prompts.borrow_mut().push(Prompt {
            window: window.clone(),
            finish,
        });
        window.present();
        if let Some(cancel) = cancel {
            cancel.grab_focus();
        }
    }

    fn workspace_prompt(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.prompt(
            "Grant automation workspace?",
            &format!("Allow automation for {}?\n\nPage text and screenshots can be sent to the MCP client and its model provider. The profile is ephemeral. Inputs can have real-world side effects. This grant expires after 10 minutes.", self.origin),
            &[("Deny", gtk::ResponseType::Cancel), ("Grant workspace", gtk::ResponseType::Accept)],
            move |response| {
                if let Some(this) = weak.upgrade() {
                    if response == gtk::ResponseType::Accept && !this.stopped.get() {
                        this.granted_until.set(Some(Instant::now() + Duration::from_secs(600)));
                    } else {
                        this.stop(StopReason::GrantDenied);
                    }
                }
            },
        );
    }

    fn authorize(self: &Rc<Self>, value: Value, reply: mpsc::SyncSender<Value>) {
        let generation = value.get("generation").and_then(Value::as_u64);
        let label = value.get("label").and_then(Value::as_str);
        let details = value.get("details").and_then(Value::as_str).filter(|text| {
            !text.is_empty()
                && text.len() <= 2048
                && text.bytes().all(|byte| (32..=126).contains(&byte))
        });
        let valid = details.is_some()
            && self.granted()
            && self.filter_ready.get()
            && generation == Some(self.generation.get())
            && matches!(
                label,
                Some("click" | "type" | "key" | "scroll" | "navigate")
            );
        if !valid || self.fixture {
            let mut result = self.reply(&value);
            result["authorized"] = json!(valid);
            let _ = reply.try_send(result);
            return;
        }
        let weak = Rc::downgrade(self);
        self.prompt(
            "Allow automation once?",
            &format!("Allow {} on {} once?\n\nTarget details (untrusted page/client data, not authority):\n{}\n\nThis interaction can have real-world side effects. The requested typed value is not included here.", label.unwrap_or("interaction"), self.origin, details.unwrap_or("Unavailable")),
            &[("Stop automation", gtk::ResponseType::Reject), ("Cancel", gtk::ResponseType::Cancel), ("Allow once", gtk::ResponseType::Accept)],
            move |response| {
                if let Some(this) = weak.upgrade() {
                    if response == gtk::ResponseType::Reject {
                        this.stop(StopReason::ActionStop);
                    }
                    let authorized = response == gtk::ResponseType::Accept
                        && this.granted()
                        && this.filter_ready.get()
                        && generation == Some(this.generation.get());
                    let mut result = this.reply(&value);
                    result["authorized"] = json!(authorized);
                    let _ = reply.try_send(result);
                }
            },
        );
    }

    fn stop(&self, reason: StopReason) {
        if self.stopped.replace(true) {
            return;
        }
        // One bounded, fixed cause code on stderr, never token/origin/target,
        // key identity, page contents or typed text. Reentrant close is silent.
        eprintln!("Automation stopped: {}", reason.label());
        self.granted_until.set(None);
        self.filter_cancel.cancel();
        self.filter_ready.set(false);
        let prompts = std::mem::take(&mut *self.prompts.borrow_mut());
        for prompt in prompts {
            (prompt.finish)(gtk::ResponseType::Cancel);
            prompt.window.close();
        }
        self.context.set_automation_allowed(false);
        if let Some(view) = self.view.borrow().as_ref() {
            view.stop_loading();
        }
        self.label.set_label("AUTOMATION STOPPED — control revoked");
        self.window.close();
        self.app.quit();
    }

    fn reply(&self, value: &Value) -> Value {
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        let op = value["op"].as_str().unwrap_or("");
        if op == "stop" {
            self.stop(StopReason::ProtocolStop);
        }
        if op == "action" {
            let label = value["label"].as_str().unwrap_or("Action");
            let label: String = label.chars().filter(|c| !c.is_control()).take(80).collect();
            self.label.set_label(&format!(
                "AUTOMATION · ephemeral · {} · {}",
                self.origin, label
            ));
            let point = value["x"]
                .as_f64()
                .zip(value["y"].as_f64())
                .filter(|(x, y)| x.is_finite() && y.is_finite() && *x >= 0.0 && *y >= 0.0);
            if point.is_some() {
                self.point.set(point);
                self.marker.queue_draw();
            }
        }
        let view = self.view.borrow();
        let origin = view
            .as_ref()
            .and_then(|v| v.uri())
            .and_then(|uri| url::Url::parse(&uri).ok())
            .filter(|url| matches!(url.scheme(), "http" | "https"))
            .map(|url| url.origin().ascii_serialization());
        json!({"id":id,"ok":true,"stopped":self.stopped.get(),"granted":self.granted(),"filter_ready":self.filter_ready.get(),"generation":self.generation.get(),"origin":origin,
            "viewport":{"width":view.as_ref().map_or(0, |v| v.width()),"height":view.as_ref().map_or(0, |v| v.height())},
            "scale":view.as_ref().map_or(1, |v| v.scale_factor()),"zoom":view.as_ref().map_or(1.0, |v| v.zoom_level()),
            "cursor":self.point.get().map(|(x,y)| json!({"x":x,"y":y}))})
    }
}

fn allowed(uri: &str, origin: &str) -> bool {
    url::Url::parse(uri).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
            && url.origin().ascii_serialization() == origin
    })
}

fn valid_origin(origin: &str, fixture: bool) -> bool {
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    if url.origin().ascii_serialization() != origin
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    if fixture {
        return matches!(url.scheme(), "http" | "https")
            && url.host_str() == Some("127.0.0.1")
            && url.port().is_some();
    }
    url.scheme() == "https"
}

fn grant_valid(until: Option<Instant>, now: Instant) -> bool {
    until.is_some_and(|deadline| now < deadline)
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(path) = self.filter_dir.borrow_mut().take() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn control(
    listener: UnixListener,
    token: String,
    tx: mpsc::SyncSender<Control>,
    idle_timeout: Duration,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(_) => {
                let _ = tx.send(Control::Closed);
                return;
            }
        }
    };
    let _ = stream.set_read_timeout(Some(idle_timeout));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream);
    'requests: loop {
        let mut line = Vec::new();
        let result = reader.by_ref().take(16385).read_until(b'\n', &mut line);
        if line.is_empty()
            && result.as_ref().is_err_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                )
            })
        {
            // An idle owner is not a disconnect. Grant expiry is enforced by
            // the GTK timer; partial/timed-out frames still fail closed below.
            continue;
        }
        if !matches!(result, Ok(1..)) || line.len() > 16384 {
            break;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            break;
        };
        if value["token"].as_str() != Some(&token)
            || !matches!(
                value["op"].as_str(),
                Some("status" | "action" | "stop" | "authorize")
            )
        {
            break;
        }
        let stop = value["op"] == "stop";
        let timeout = if value["op"] == "authorize" { 120 } else { 5 };
        let (reply, receive) = mpsc::sync_channel(1);
        if tx.send(Control::Request(value, reply)).is_err() {
            break;
        }
        let result = if timeout == 120 {
            // The protocol is sequential. While a native decision is pending,
            // EOF or pipelined bytes revoke the owner rather than leaving an
            // apparently approved dialog alive after the client has gone away.
            if !reader.buffer().is_empty() {
                break;
            }
            let _ = reader
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(100)));
            let deadline = Instant::now() + Duration::from_secs(timeout);
            let result = loop {
                match receive.try_recv() {
                    Ok(value) => break value,
                    Err(mpsc::TryRecvError::Disconnected) => break 'requests,
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                if Instant::now() >= deadline {
                    break 'requests;
                }
                let mut byte = [0u8; 1];
                match reader.get_mut().read(&mut byte) {
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    _ => break 'requests,
                }
            };
            let _ = reader.get_mut().set_read_timeout(Some(idle_timeout));
            result
        } else {
            let Ok(result) = receive.recv_timeout(Duration::from_secs(timeout)) else {
                break;
            };
            result
        };
        let Ok(mut output) = serde_json::to_vec(&result) else {
            break;
        };
        output.push(b'\n');
        if reader.get_mut().write_all(&output).is_err() || stop {
            break;
        }
    }
    let _ = tx.send(Control::Closed);
}

pub fn run() -> glib::ExitCode {
    let configuration = (|| -> Result<_, String> {
        if std::env::args().skip(1).collect::<Vec<_>>() != ["--automation"]
            || std::env::var("PAISLEY_MCP_SANDBOX").as_deref() != Ok("1")
        {
            return Err("Automation requires the isolated MCP launcher".into());
        }
        let socket =
            std::env::var("PAISLEY_AUTOMATION_SOCKET").map_err(|_| "Missing control socket")?;
        let token =
            std::env::var("PAISLEY_AUTOMATION_TOKEN").map_err(|_| "Missing control token")?;
        let origin =
            std::env::var("PAISLEY_AUTOMATION_ORIGIN").map_err(|_| "Missing origin grant")?;
        let proxy =
            std::env::var("PAISLEY_AUTOMATION_PROXY").map_err(|_| "Missing isolated proxy")?;
        let fixture = match std::env::var("PAISLEY_AUTOMATION_FIXTURE") {
            Ok(value) if value == "1" => true,
            Err(std::env::VarError::NotPresent) => false,
            _ => return Err("Invalid fixture flag".into()),
        };
        if !valid_origin(&origin, fixture)
            || token.len() != 64
            || !token.bytes().all(|b| b.is_ascii_hexdigit())
            || !socket.starts_with("/tmp/control-")
        {
            return Err("Invalid isolated workspace configuration".into());
        }
        Ok((socket, token, origin, proxy, fixture))
    })();
    let (socket, token, origin, proxy, fixture) = match configuration {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return glib::ExitCode::FAILURE;
        }
    };
    let listener = match UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("Control bind failed: {error}");
            return glib::ExitCode::FAILURE;
        }
    };
    let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));
    let _ = listener.set_nonblocking(true);
    let (tx, rx) = mpsc::sync_channel(2);
    std::thread::spawn(move || control(listener, token, tx, Duration::from_secs(125)));
    let app = gtk::Application::builder()
        .application_id("site.blig.Paisley.Automation")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let receiver = Rc::new(RefCell::new(Some(rx)));
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let this = Runtime::build(app, origin.clone(), proxy.clone(), fixture);
        let Some(rx) = receiver.borrow_mut().take() else {
            return;
        };
        glib::timeout_add_local(Duration::from_millis(16), move || {
            if this.stopped.get() {
                return glib::ControlFlow::Break;
            }
            if this.granted_until.get().is_some() && !this.granted() {
                this.stop(StopReason::GrantExpired);
                return glib::ControlFlow::Break;
            }
            match rx.try_recv() {
                Ok(Control::Request(value, reply)) => {
                    if value["op"] == "authorize" {
                        this.authorize(value, reply);
                    } else {
                        let _ = reply.try_send(this.reply(&value));
                    }
                }
                Ok(Control::Closed) | Err(mpsc::TryRecvError::Disconnected) => {
                    this.stop(StopReason::ControlClosed);
                    return glib::ControlFlow::Break;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            glib::ControlFlow::Continue
        });
    });
    let result = app.run_with_args(&["paisley-automation"]);
    let _ = std::fs::remove_file(socket);
    result
}

/// Called by the existing single-threaded GTK smoke test, never a standalone GTK test.
#[cfg(test)]
pub(crate) fn check_native_grants(app: &gtk::Application, capture: impl Fn(&gtk::Window, &str)) {
    fn button(root: &gtk::Widget, label: &str) -> Option<gtk::Button> {
        if let Some(button) = root.downcast_ref::<gtk::Button>()
            && button.label().as_deref() == Some(label)
        {
            return Some(button.clone());
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Some(found) = button(&widget, label) {
                return Some(found);
            }
            child = widget.next_sibling();
        }
        None
    }
    fn label_contains(root: &gtk::Widget, text: &str) -> bool {
        if root
            .downcast_ref::<gtk::Label>()
            .is_some_and(|label| label.text().contains(text))
        {
            return true;
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            if label_contains(&widget, text) {
                return true;
            }
            child = widget.next_sibling();
        }
        false
    }
    fn click(runtime: &Runtime, label: &str) {
        let window = runtime.prompts.borrow().last().unwrap().window.clone();
        button(window.upcast_ref(), label)
            .expect("native consent button")
            .emit_clicked();
    }
    // Never navigate or connect: only trusted native widgets and actual local
    // filter compilation are exercised, on the existing single GTK thread.
    let runtime = Runtime::build(
        app,
        "https://owned.example".into(),
        "http://127.0.0.1:9".into(),
        false,
    );
    assert!(!runtime.granted());
    let workspace = runtime.prompts.borrow().last().unwrap().window.clone();
    capture(&workspace, "PAISLEY_GRANT_SCREENSHOT");
    assert_eq!(
        runtime.reply(&json!({"op":"action","label":"Grant workspace"}))["granted"],
        false
    );
    let request = json!({"id":1,"op":"authorize","label":"click","generation":0,"details":"Target: #send; label=Preview"});
    let (tx, rx) = mpsc::sync_channel(1);
    runtime.authorize(request.clone(), tx);
    assert_eq!(rx.try_recv().unwrap()["authorized"], false);
    click(&runtime, "Grant workspace");
    assert!(runtime.granted());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !runtime.filter_ready.get() && Instant::now() < deadline {
        let context = glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        runtime.filter_ready.get(),
        "native exact-origin filter did not compile"
    );
    for label in ["click", "type", "key", "scroll", "navigate"] {
        let request = json!({"id":1,"op":"authorize","label":label,"generation":0,"details":"Target: #send; label=Preview"});
        let (tx, rx) = mpsc::sync_channel(1);
        runtime.authorize(request.clone(), tx);
        let prompt = runtime.prompts.borrow().last().unwrap().window.clone();
        if label == "click" {
            capture(&prompt, "PAISLEY_CONSENT_SCREENSHOT");
        }
        assert!(
            label_contains(prompt.upcast_ref(), "Target: #send; label=Preview"),
            "native approval hid the requested target"
        );
        click(&runtime, "Cancel");
        assert_eq!(rx.try_recv().unwrap()["authorized"], false);
        let (tx, rx) = mpsc::sync_channel(1);
        runtime.authorize(request, tx);
        click(&runtime, "Allow once");
        assert_eq!(rx.try_recv().unwrap()["authorized"], true);
    }
    let (tx, rx) = mpsc::sync_channel(1);
    runtime.authorize(request.clone(), tx);
    runtime.generation.set(1);
    click(&runtime, "Allow once");
    assert_eq!(rx.try_recv().unwrap()["authorized"], false);
    runtime
        .granted_until
        .set(Some(Instant::now() - Duration::from_secs(1)));
    let (tx, rx) = mpsc::sync_channel(1);
    runtime.authorize(request, tx);
    assert_eq!(rx.try_recv().unwrap()["authorized"], false);
    runtime
        .granted_until
        .set(Some(Instant::now() + Duration::from_secs(600)));
    let (tx, rx) = mpsc::sync_channel(1);
    runtime.authorize(
        json!({"id":2,"op":"authorize","label":"click","generation":1,"details":"Target: #send; label=Preview"}),
        tx,
    );
    click(&runtime, "Stop automation");
    assert_eq!(rx.try_recv().unwrap()["authorized"], false);
    assert!(runtime.stopped.get());
    assert!(!runtime.granted());
    let denied = Runtime::build(
        app,
        "https://owned.example".into(),
        "http://127.0.0.1:9".into(),
        false,
    );
    click(&denied, "Deny");
    assert!(!denied.granted());
    assert!(denied.stopped.get());
    // Do not terminate GTK's test thread with a cancelled WebKit GTask still
    // queued. Production owns the runtime until this completion callback too.
    let deadline = Instant::now() + Duration::from_secs(5);
    while denied.filter_pending.get() && Instant::now() < deadline {
        let context = glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !denied.filter_pending.get(),
        "cancelled native filter task did not finish"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_diagnostics_are_fixed_bounded_cause_codes() {
        for (reason, expected) in [
            (StopReason::StopButton, "stop-button"),
            (StopReason::NativeKey, "native-key-preemption"),
            (StopReason::NativePointer, "native-pointer-preemption"),
            (StopReason::WindowClosed, "window-closed"),
            (StopReason::MissingSession, "automation-session-unavailable"),
            (
                StopReason::NonEphemeralSession,
                "non-ephemeral-session-refused",
            ),
            (StopReason::GrantDenied, "workspace-grant-denied"),
            (StopReason::ActionStop, "action-consent-stop"),
            (StopReason::ProtocolStop, "protocol-stop"),
            (StopReason::GrantExpired, "workspace-grant-expired"),
            (StopReason::ControlClosed, "control-owner-disconnected"),
        ] {
            assert_eq!(reason.label(), expected);
            assert!(reason.label().len() <= 64 && reason.label().is_ascii());
        }
    }

    #[test]
    fn canonical_origins_and_fixture_scope() {
        assert!(valid_origin("https://example.com", false));
        assert!(valid_origin("https://example.com:8443", false));
        assert!(valid_origin("http://127.0.0.1:8080", true));
        assert!(valid_origin("https://127.0.0.1:8443", true));
        for origin in [
            "http://example.com",
            "https://example.com/",
            "https://user@example.com",
            "https://example.com?x=1",
            "https://example.com#x",
            "https://example.com:443",
        ] {
            assert!(!valid_origin(origin, false), "{origin}");
        }
        for origin in [
            "https://example.com:8443",
            "http://localhost:8080",
            "http://127.0.0.1",
            "https://127.0.0.1:443",
        ] {
            assert!(!valid_origin(origin, true), "{origin}");
        }
        assert!(!valid_origin("http://127.0.0.1:8080", false));
    }

    #[test]
    fn idle_control_timeout_does_not_revoke_a_live_connection() {
        use std::os::unix::net::UnixStream;
        let path = std::env::temp_dir().join(format!(
            "paisley-idle-control-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let listener = UnixListener::bind(&path).unwrap();
        let (tx, rx) = mpsc::sync_channel(2);
        let worker = std::thread::spawn(move || {
            control(listener, "fixture".into(), tx, Duration::from_millis(20))
        });
        let mut client = UnixStream::connect(&path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        std::thread::sleep(Duration::from_millis(80));
        assert!(
            matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "ordinary idle timeout revoked the workspace"
        );
        client
            .write_all(b"{\"id\":1,\"op\":\"status\",\"token\":\"fixture\"}\n")
            .unwrap();
        let Control::Request(value, reply) = rx.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("idle client disconnected")
        };
        assert_eq!(value["id"], 1);
        reply.send(json!({"id":1,"ok":true})).unwrap();
        let mut line = String::new();
        BufReader::new(client.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["ok"], true);
        drop(client);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Control::Closed
        ));
        worker.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn grant_expiry_and_revocation_are_fail_closed() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(600);
        assert!(grant_valid(Some(deadline), now));
        assert!(!grant_valid(Some(deadline), deadline));
        assert!(!grant_valid(None, now));
    }

    #[test]
    fn navigation_requires_exact_origin_without_credentials() {
        assert!(allowed(
            "https://example.com/path?q=1",
            "https://example.com"
        ));
        assert!(!allowed(
            "https://example.com.evil.test/",
            "https://example.com"
        ));
        assert!(!allowed("https://user@example.com/", "https://example.com"));
        assert!(!allowed("http://example.com/", "https://example.com"));
        assert!(!allowed("file:///etc/passwd", "https://example.com"));
    }
}
