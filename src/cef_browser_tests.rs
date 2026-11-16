use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn pump() {
    let context = gtk::glib::MainContext::default();
    for _ in 0..128 {
        if !context.pending() {
            break;
        }
        context.iteration(false);
    }
}
fn wait_until(description: &str, check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !check() {
        assert!(Instant::now() < deadline, "timed out: {description}");
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
}
struct Fixture {
    base: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let cross_base = format!("http://localhost:{}", listener.local_addr().unwrap().port());
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut request = [0; 16384];
                        let count = stream.read(&mut request).unwrap_or(0);
                        let request = String::from_utf8_lossy(&request[..count]);
                        let path = request.split_whitespace().nth(1).unwrap_or("/");
                        if path == "/cross-set" || path.starts_with("/cross-read") {
                            let title = if path == "/cross-set" {
                                "Cross seeded"
                            } else if request.contains("paisley_cross=shared") {
                                "Cross shared"
                            } else if request.contains("paisley_cross=changed") {
                                "Cross changed"
                            } else {
                                "Cross missing"
                            };
                            let cookie = if path == "/cross-set" {
                                "Set-Cookie: paisley_cross=shared; Path=/; SameSite=None; Secure\r\n"
                            } else if path.contains("?set=changed") {
                                "Set-Cookie: paisley_cross=changed; Path=/; SameSite=None; Secure\r\n"
                            } else {
                                ""
                            };
                            let body = format!(
                                "<!doctype html><title>{title}</title><script>parent.postMessage('{title}','*')</script>"
                            );
                            let header = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n{cookie}Content-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(body.as_bytes());
                            continue;
                        }
                        if path.starts_with("/auth") {
                            let accepted = path == "/auth/accepted"
                                && request.contains("Authorization: Basic dm94OmZpeHR1cmU=");
                            let (status, extra, body) = if accepted {
                                (
                                    "200 OK",
                                    "",
                                    "<!doctype html><title>Authentication accepted</title>Authenticated fixture",
                                )
                            } else {
                                (
                                    "401 Unauthorized",
                                    "WWW-Authenticate: Basic realm=\"Paisley fixture\"\r\n",
                                    "<!doctype html><title>Authentication cancelled</title>Authentication required",
                                )
                            };
                            let header = format!(
                                "HTTP/1.1 {status}\r\nContent-Type: text/html\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                body.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            continue;
                        }
                        if path == "/drop" {
                            continue;
                        }
                        if path == "/favicon.png" {
                            let data = include_bytes!("../tests/fixtures/downloads/sample.png");
                            let header = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                data.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(data);
                            continue;
                        }
                        if path.starts_with("/download/") {
                            let name = path.trim_start_matches("/download/");
                            let slow = name == "slow.bin";
                            let payload: Vec<u8> = match name {
                                "sample.png" => {
                                    include_bytes!("../tests/fixtures/downloads/sample.png")
                                        .to_vec()
                                }
                                "sample.zip" => {
                                    include_bytes!("../tests/fixtures/downloads/sample.zip")
                                        .to_vec()
                                }
                                "sample.pdf" => {
                                    include_bytes!("../tests/fixtures/downloads/sample.pdf")
                                        .to_vec()
                                }
                                "sample.txt" => {
                                    include_bytes!("../tests/fixtures/downloads/sample.txt")
                                        .to_vec()
                                }
                                _ => vec![90; 2 * 1024 * 1024],
                            };
                            let size = payload.len();
                            if !request.contains("paisley_native=shared") {
                                let _=stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                                continue;
                            }
                            let header = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{name}\"\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n"
                            );
                            let _ = stream.write_all(header.as_bytes());
                            for chunk in payload.chunks(4096) {
                                if stream.write_all(chunk).is_err() {
                                    break;
                                }
                                if slow {
                                    std::thread::sleep(Duration::from_millis(20));
                                }
                            }
                            continue;
                        }
                        let (title, cookie) = if path == "/read" {
                            (
                                if request.contains("paisley_native=shared") {
                                    "Cookie shared"
                                } else {
                                    "Cookie missing"
                                },
                                "",
                            )
                        } else if path == "/set" {
                            (
                                "Cookie set",
                                "Set-Cookie: paisley_native=shared; Path=/; HttpOnly; SameSite=Lax\r\n",
                            )
                        } else {
                            ("Paisley native fixture", "")
                        };
                        let mut body = format!(
                            "<!doctype html><meta charset=utf-8><link rel=icon href=/favicon.png><title>{title}</title><style>body{{background:#182030;color:#fff;font:24px sans-serif}}</style><h1>日本語 中文 🦋</h1><input id=draft value='keep me'>"
                        );
                        if path == "/cross" || path == "/cross-write" {
                            body.push_str(&format!("<script>addEventListener('message',e=>{{if(e.origin==='{cross_base}'&&typeof e.data==='string'&&e.data.startsWith('Cross '))document.title=e.data}})</script><iframe src='{cross_base}/cross-read{}'></iframe>",if path=="/cross-write"{"?set=changed"}else{""}));
                        }
                        if path == "/select" {
                            body.push_str("<select style='position:fixed;left:20px;top:20px;width:300px;height:40px' onchange=\"document.title='Selected:'+this.value\"><option value=one>First option</option><option value=two>日本語 中文 🦋</option><option value=three>Third option</option></select>");
                        }
                        if path == "/file" {
                            let expected = serde_json::to_string(include_str!(
                                "../tests/fixtures/downloads/sample.txt"
                            ))
                            .unwrap();
                            body.push_str(&format!("<input id=upload type=file accept='.txt' style='position:fixed;left:20px;top:20px;width:300px;height:100px'><script>document.getElementById('upload').onchange=event=>{{const f=event.target.files[0];f.text().then(text=>document.title=f.name==='sample.txt'&&text==={expected}?'Uploaded exact fixture':'Upload mismatch')}}</script>"));
                        }
                        if path == "/privacy" {
                            body.push_str("<script>addEventListener('message',e=>{if(e.origin===location.origin&&e.data==='undefined'&&typeof RTCPeerConnection==='undefined')document.title='WebRTC disabled in both frames'})</script><iframe src=/privacy-frame></iframe>");
                        }
                        if path == "/privacy-frame" {
                            body.push_str("<script>parent.postMessage(typeof RTCPeerConnection,location.origin)</script>");
                        }
                        if let Some(number) = path
                            .strip_prefix("/memory")
                            .filter(|n| n.parse::<usize>().is_ok())
                        {
                            body.push_str(&format!("<script>window.testHeap=new Uint8Array(32*1024*1024);window.testHeap.fill(123);document.title='Memory {number}';</script>"));
                        }
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n{cookie}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self {
            base,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub(crate) fn run() -> gtk::glib::ExitCode {
    gtk::init().unwrap();
    let fixture = Fixture::new();
    let app = gtk::Application::builder()
        .application_id("site.blig.Paisley.EngineTests")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    let display = gtk::gdk::Display::default().unwrap();
    fonts_rs_lucide::Lucide::install_icon_theme(&display).unwrap();
    crate::download_ui::install_styles(&display);
    if let Ok(url) = std::env::var("PAISLEY_NATIVE_URL") {
        return public_video(&app, &url);
    }
    let mut profile = paisley::profile::Profile::open("native-engine-test", true).unwrap();
    profile.settings.homepage = fixture.base.clone();
    let browser = Browser::with_profile(&app, profile).unwrap();
    browser.present();
    let first = browser.active_tab().unwrap();
    wait_until("native homepage title", || {
        first.view.title().as_deref() == Some("Paisley native fixture")
    });
    wait_until("native favicon", || first.view.favicon().is_some());
    wait_until("native privacy preferences applied", || {
        browser.native_privacy_ready.get().is_some()
    });
    assert_eq!(browser.native_privacy_ready.get(), Some(true));
    if std::env::var_os("PAISLEY_NATIVE_POPUP").is_some() {
        select_popup_lifecycle(&browser, &fixture);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }
    if std::env::var_os("PAISLEY_NATIVE_FILE").is_some() {
        file_lifecycle(&browser, &fixture);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }
    if std::env::var_os("PAISLEY_NATIVE_AUTH").is_some() {
        auth_lifecycle(&browser, &fixture);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }
    if std::env::var_os("PAISLEY_NATIVE_PUBLIC_MEMORY").is_some() {
        public_memory_lifecycle(&browser);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }
    if std::env::var_os("PAISLEY_NATIVE_DIALOGS").is_some() {
        script_dialog_lifecycle(&browser, &fixture);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }
    if std::env::var_os("PAISLEY_NATIVE_PRIVACY").is_some() {
        privacy_lifecycle(&browser, &fixture);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }

    if std::env::var_os("PAISLEY_NATIVE_STORAGE").is_some() {
        browser.navigate(&format!("{}/set", fixture.base));
        wait_until("storage cookie seeded", || {
            first.view.title().as_deref() == Some("Cookie set")
        });
        clear_site_data(&browser, &fixture);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }
    if std::env::var_os("PAISLEY_NATIVE_ERRORS").is_some() {
        navigation_failure(&browser, &fixture);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }
    if std::env::var_os("PAISLEY_NATIVE_TOOLS").is_some() {
        inspector_lifecycle(&browser, &fixture);
        browser.window.close();
        return gtk::glib::ExitCode::SUCCESS;
    }
    first
        .view
        .evaluate_javascript("document.getElementById('draft').value='edited draft'");
    let mut cycle_memory = Vec::new();
    for cycle in 0..60 {
        let view = browser.add_tab(None, None);
        assert!(
            !view.has_renderer(),
            "unloaded tab must not start an engine"
        );
        browser.navigate(&format!("{}/cycle{cycle}", fixture.base));
        wait_until("lifecycle page", || {
            view.title().as_deref() == Some("Paisley native fixture")
        });
        assert_eq!(
            view.renderer_identifier(),
            first.view.renderer_identifier(),
            "one engine per profile"
        );
        let id = browser.active_tab().unwrap().id;
        browser.close_tab(id);
        assert!(
            !view.has_renderer(),
            "close releases native handle even if GTK view is retained"
        );
        if (cycle + 1) % 12 == 0 {
            let until = Instant::now() + Duration::from_secs(2);
            while Instant::now() < until {
                pump();
                std::thread::sleep(Duration::from_millis(10));
            }
            let usage = paisley::memory::sample(std::process::id()).unwrap();
            eprintln!(
                "CYCLE_MEMORY after {} closed tabs: {} MiB {} processes ({})",
                cycle + 1,
                usage.kib / 1024,
                usage.processes,
                usage.metric
            );
            cycle_memory.push(usage.kib);
        }
    }
    let start = cycle_memory[0];
    let end = *cycle_memory.last().unwrap();
    assert!(
        end <= start + (128 * 1024).max(start / 4),
        "memory kept growing across tab cycles: {}→{} MiB",
        start / 1024,
        end / 1024
    );
    first.view.evaluate_javascript("document.title=document.getElementById('draft').value==='edited draft'?'Draft kept':'Draft lost'");
    wait_until("form state retained", || {
        first.view.title().as_deref() == Some("Draft kept")
    });
    browser.navigate(&format!("{}/set", fixture.base));
    wait_until("cookie set", || {
        first.view.title().as_deref() == Some("Cookie set")
    });
    let second = browser.add_tab(None, Some(&format!("{}/read", fixture.base)));
    wait_until("same profile cookie sharing", || {
        second.title().as_deref() == Some("Cookie shared")
    });
    let mut profile = paisley::profile::Profile::open("native-isolation", true).unwrap();
    profile.settings.homepage = format!("{}/read", fixture.base);
    let other = Browser::with_profile(&app, profile).unwrap();
    other.present();
    let isolated = other.active_tab().unwrap();
    wait_until("private cookie isolation", || {
        isolated.view.title().as_deref() == Some("Cookie missing")
    });
    assert_ne!(
        isolated.view.renderer_identifier(),
        first.view.renderer_identifier()
    );
    other.window.close();
    drop(other);
    #[cfg(feature = "cef")]
    memory_lifecycle(&browser, &fixture);
    #[cfg(feature = "cef")]
    download_lifecycle(&browser, &fixture);
    navigation_failure(&browser, &fixture);
    clear_site_data(&browser, &fixture);
    privacy_lifecycle(&browser, &fixture);
    inspector_lifecycle(&browser, &fixture);
    script_dialog_lifecycle(&browser, &fixture);
    auth_lifecycle(&browser, &fixture);
    select_popup_lifecycle(&browser, &fixture);
    file_cancellation_lifecycle(&browser, &fixture);
    crash_recovery(&browser, &fixture);
    browser.window.close();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    eprintln!(
        "Native engine: 60 tab cycles, form retention, cookie sharing and private isolation passed"
    );
    gtk::glib::ExitCode::SUCCESS
}

fn public_video(app: &gtk::Application, url: &str) -> gtk::glib::ExitCode {
    let parsed = url::Url::parse(url).unwrap();
    assert!(matches!(parsed.scheme(), "https" | "http"));
    let mut profile = paisley::profile::Profile::open("native-video-test", true).unwrap();
    profile.settings.homepage = url.into();
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    let view = browser.active_tab().unwrap().view;
    let start = Instant::now();
    let mut probe = Instant::now();
    let mut previous = None;
    let mut success = false;
    while start.elapsed() < Duration::from_secs(120) && !browser.closing.get() {
        pump();
        std::thread::sleep(Duration::from_millis(10));
        if probe.elapsed() > Duration::from_secs(5) {
            if let Some(title) = view.title() {
                if let Some(value) = title
                    .strip_prefix("PAISLEY_VIDEO:")
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                {
                    eprintln!("YouTube video probe: {value}");
                    if value["paused"] == false && value["ad"] == false && value["error"].is_null()
                    {
                        if let Some(time) = value["time"].as_f64() {
                            if previous.is_some_and(|old| time > old + 3.0) {
                                success = true;
                                break;
                            }
                            previous.get_or_insert(time);
                        }
                    }
                }
            }
            view.evaluate_javascript("(()=>{const v=document.querySelector('video');document.title='PAISLEY_VIDEO:'+JSON.stringify(v?{time:v.currentTime,paused:v.paused,error:v.error?.code??null,ad:!!document.querySelector('.ad-showing')}:{found:false})})()");
            probe = Instant::now();
        }
    }
    browser.window.close();
    eprintln!("YouTube actual playback verified: {success}");
    if success {
        gtk::glib::ExitCode::SUCCESS
    } else {
        gtk::glib::ExitCode::FAILURE
    }
}

#[cfg(feature = "cef")]
fn memory_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    let mut heavy = Vec::new();
    for n in 0..8 {
        let tab = if n < 2 {
            browser.tabs.borrow()[n].clone()
        } else {
            browser.add_tab(None, None);
            browser.active_tab().unwrap()
        };
        browser.load_tab(&tab, &format!("{}/memory{n}", fixture.base));
        wait_until("32 MiB memory fixture loaded", || {
            tab.view.title().as_deref() == Some(format!("Memory {n}").as_str())
        });
        heavy.push(tab);
    }
    heavy[0]
        .view
        .evaluate_javascript("document.getElementById('draft').value='protected draft'");
    wait_until("all frame activity probes", || {
        heavy.iter().enumerate().all(|(n, t)| {
            t.view
                .page_activity()
                .is_some_and(|(_, protected)| protected == (n == 0))
        })
    });
    memory_stage("eight resident tabs with 32 MiB each");
    browser
        .residency_epoch
        .set(Instant::now() - Duration::from_secs(400));
    for tab in &heavy {
        tab.last_used.set(Instant::now() - Duration::from_secs(350));
    }
    *browser.memory.borrow_mut() = paisley::memory::sample(std::process::id());
    browser.reclaim_idle_tabs();
    let sleeping = heavy.iter().filter(|t| t.discarded.get()).count();
    assert!(
        sleeping >= 5,
        "at least five inactive tabs should sleep: {sleeping}"
    );
    assert!(
        heavy[0].view.has_renderer(),
        "dirty form must stay resident"
    );
    assert!(
        heavy[7].view.has_renderer(),
        "active tab must stay resident"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    memory_stage("after inactive tabs slept");
    let restored = heavy.iter().find(|t| t.discarded.get()).unwrap().clone();
    restored.committed.set(false);
    browser
        .notebook
        .set_current_page(browser.notebook.page_num(&restored.page));
    wait_until("sleeping tab restores", || {
        restored.view.has_renderer() && restored.committed.get()
    });
    heavy[0].view.evaluate_javascript("document.title=document.getElementById('draft').value==='protected draft'?'Protected draft kept':'Protected draft lost'");
    wait_until("protected draft survived reclamation", || {
        heavy[0].view.title().as_deref() == Some("Protected draft kept")
    });
    eprintln!("Live tab residency: {sleeping} tabs slept; restore and protected draft passed");
}
#[cfg(feature = "cef")]
fn memory_stage(stage: &str) {
    if let Some(usage) = paisley::memory::sample(std::process::id()) {
        eprintln!(
            "MEMORY STAGE {stage}: {} MiB {} processes ({})",
            usage.kib / 1024,
            usage.processes,
            usage.metric
        );
    }
}

#[cfg(feature = "cef")]
fn download_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    let directory =
        std::env::temp_dir().join(format!("paisley-cef-download-test-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let manager = browser.downloads.borrow().native_test_manager();
    manager.set_directory(directory.clone());
    let source = browser.add_tab(None, Some(&format!("{}/read", fixture.base)));
    let source_id = browser.active_tab().unwrap().id;
    wait_until("download source ready", || {
        source.title().as_deref() == Some("Cookie shared")
    });
    let session = browser.native_session.borrow().clone().unwrap();
    session.send(cef_gtk::proto_ipc::servo_action::Action::StartDownload(
        cef_gtk::proto_ipc::StartDownload {
            page_id: source.page_id().unwrap(),
            url: format!("{}/download/slow.bin", fixture.base),
        },
    ));
    wait_until("native download received bytes", || {
        manager
            .snapshots()
            .iter()
            .any(|(_, received, _, terminal)| *received > 0 && !*terminal)
    });
    let id = manager
        .snapshots()
        .into_iter()
        .find(|(_, received, _, terminal)| *received > 0 && !*terminal)
        .unwrap()
        .0;
    manager.test_control(id, "pause");
    wait_until("native download paused", || {
        manager
            .snapshots()
            .iter()
            .any(|(n, _, paused, terminal)| *n == id && *paused && !*terminal)
    });
    let received = manager
        .snapshots()
        .into_iter()
        .find(|(n, _, _, _)| *n == id)
        .unwrap()
        .1;
    assert!(received > 0, "pause retains received bytes");
    browser.close_tab(source_id);
    manager.test_control(id, "resume");
    wait_until("download completes after source tab close", || {
        browser
            .downloads
            .borrow()
            .completed_paths()
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "slow.bin"))
    });
    let path = browser
        .downloads
        .borrow()
        .completed_paths()
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == "slow.bin"))
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), vec![90; 2 * 1024 * 1024]);
    for name in ["sample.png", "sample.zip", "sample.txt", "sample.pdf"] {
        let previous = browser.downloads.borrow().completed_paths().len();
        session.send(cef_gtk::proto_ipc::servo_action::Action::StartDownload(
            cef_gtk::proto_ipc::StartDownload {
                page_id: 0,
                url: format!("{}/download/{name}", fixture.base),
            },
        ));
        wait_until("file type completes", || {
            browser.downloads.borrow().completed_paths().len() > previous
        });
        let path = browser
            .downloads
            .borrow()
            .completed_paths()
            .last()
            .unwrap()
            .clone();
        let expected = match name {
            "sample.png" => include_bytes!("../tests/fixtures/downloads/sample.png").as_slice(),
            "sample.zip" => include_bytes!("../tests/fixtures/downloads/sample.zip").as_slice(),
            "sample.pdf" => include_bytes!("../tests/fixtures/downloads/sample.pdf").as_slice(),
            _ => include_bytes!("../tests/fixtures/downloads/sample.txt").as_slice(),
        };
        assert_eq!(std::fs::read(&path).unwrap(), expected);
        assert!(
            !std::fs::read_dir(path.parent().unwrap())
                .unwrap()
                .flatten()
                .any(|p| p
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".paisley-native")),
            "completed transfer removes staging directory"
        );
    }
    let hidden = std::fs::read_dir(&directory).unwrap().flatten().any(|p| {
        p.file_name()
            .to_string_lossy()
            .starts_with(".paisley-native")
    });
    assert!(!hidden, "successful downloads leave no staging directories");
    eprintln!(
        "Native downloads: authenticated bytes, pause/resume, closed source tab, PNG/ZIP/PDF/TXT and atomic publication passed"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(feature = "cef")]
fn crash_recovery(browser: &Rc<Browser>, fixture: &Fixture) {
    let session = browser.native_session.borrow().clone().unwrap();
    let old = browser.active_tab().unwrap().view.renderer_identifier();
    session.terminate_for_test();
    wait_until("profile engine disconnected", || session.is_closed());
    browser.navigate(&format!("{}/after-crash", fixture.base));
    let view = browser.active_tab().unwrap().view;
    wait_until("page recovered after engine crash", || {
        view.title().as_deref() == Some("Paisley native fixture")
            && view.renderer_identifier() != old
    });
    browser.native_stats.set(None);
    browser
        .native_session
        .borrow()
        .as_ref()
        .unwrap()
        .query_stats();
    wait_until("profile services reconnected", || {
        browser.native_stats.get().is_some()
    });
    eprintln!("Engine crash recovery: page and profile services reconnected");
}

#[cfg(feature = "cef")]
fn inspector_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    let view = browser.add_tab(None, Some(&format!("{}/inspector", fixture.base)));
    wait_until("inspector source fixture loaded", || {
        view.title().as_deref() == Some("Paisley native fixture")
    });
    let tab = browser.active_tab().unwrap();
    let split = tab
        .page
        .last_child()
        .unwrap()
        .downcast::<gtk::Paned>()
        .unwrap();
    gtk::gio::prelude::ActionGroupExt::activate_action(&browser.window, "inspector", None);
    wait_until("CEF inspector docked on the right", || {
        split.end_child().is_some()
    });
    let frontend = split.end_child().unwrap().downcast::<View>().unwrap();
    let relay = std::cell::RefCell::new(None);
    wait_until("CEF developer tools frontend loaded", || {
        frontend.title().is_some_and(|s| s.contains("DevTools"))
    });
    wait_until("developer tools actually painted", || {
        frontend.has_painted_frame()
    });
    assert!(frontend.has_renderer());
    *relay.borrow_mut() = frontend
        .uri()
        .and_then(|uri| url::Url::parse(&uri).ok())
        .and_then(|url| {
            url.query_pairs()
                .find(|(k, _)| k == "ws")
                .map(|(_, v)| v.split('/').next().unwrap().to_owned())
        });
    let probe = std::cell::Cell::new(Instant::now() - Duration::from_secs(1));
    wait_until("inspector reads the live page DOM", || {
        if frontend.title().as_deref() == Some("Inspector connected") {
            return true;
        }
        if probe.get().elapsed() > Duration::from_millis(500) {
            frontend.evaluate_javascript("(()=>{let text='';let count=0;function walk(n){if(++count>10000)return;if(n.nodeType===3)text+=n.textContent;if(n.shadowRoot)walk(n.shadowRoot);for(const c of n.childNodes)walk(c)}walk(document.body);if(text.includes('日本語 中文'))document.title='Inspector connected'})()");
            probe.set(Instant::now());
        }
        false
    });
    if std::env::var_os("PAISLEY_NATIVE_TOOLS").is_some() {
        let until = Instant::now() + Duration::from_secs(15);
        while Instant::now() < until {
            pump();
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    gtk::gio::prelude::ActionGroupExt::activate_action(&browser.window, "inspector", None);
    wait_until("CEF inspector closed", || split.end_child().is_none());
    assert!(!frontend.has_renderer());
    let address = relay.borrow().clone().expect("private inspector relay");
    wait_until("closed inspector releases loopback listener", || {
        std::net::TcpStream::connect(&address).is_err()
    });
    tab.view
        .evaluate_javascript("document.title='Page survives inspector close'");
    wait_until("inspected page survives closing DevTools", || {
        tab.view.title().as_deref() == Some("Page survives inspector close")
    });
    eprintln!("CEF inspector: loaded in right pane and closed without retained native handle");
}

#[cfg(feature = "cef")]
fn navigation_failure(browser: &Rc<Browser>, fixture: &Fixture) {
    browser.add_tab(None, Some(&format!("{}/drop", fixture.base)));
    let tab = browser.active_tab().unwrap();
    wait_until("navigation error shown in the browser", || {
        tab.error.is_visible()
    });
    assert!(!tab.error.text().is_empty());
    browser.close_tab(tab.id);
    eprintln!("Native navigation failure: visible error instead of an unexplained blank page");
}

#[cfg(feature = "cef")]
fn clear_site_data(browser: &Rc<Browser>, fixture: &Fixture) {
    let view = browser.add_tab(None, Some(&format!("{}/read", fixture.base)));
    wait_until("cookie exists before clearing", || {
        view.title().as_deref() == Some("Cookie shared")
    });
    view.evaluate_javascript(
        "localStorage.setItem('paisley-storage-test','present');document.title='Storage seeded'",
    );
    wait_until("native local storage seeded", || {
        view.title().as_deref() == Some("Storage seeded")
    });
    browser.request_native_clear(view.page_id().unwrap(), fixture.base.clone());
    wait_until("native site data operation finished", || {
        !browser.site_clear_pending.get()
    });
    view.reload();
    wait_until("CEF cookies removed", || {
        view.title().as_deref() == Some("Cookie missing")
    });
    view.evaluate_javascript("document.title=localStorage.getItem('paisley-storage-test')===null?'Storage cleared':'Storage retained'");
    wait_until("CEF local storage removed", || {
        view.title().as_deref() == Some("Storage cleared")
    });
    browser.close_tab(browser.active_tab().unwrap().id);
    eprintln!("CEF site data: HttpOnly cookie and local storage cleared in the correct engine");
}

#[cfg(feature = "cef")]
fn privacy_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    let source = browser.active_tab().unwrap();
    source.view.evaluate_javascript(
        "document.title=typeof RTCPeerConnection==='function'?'WebRTC available':'WebRTC missing'",
    );
    wait_until("WebRTC normal mode", || {
        source.view.title().as_deref() == Some("WebRTC available")
    });
    let cross = fixture.base.replace("127.0.0.1", "localhost");
    browser.navigate(&format!("{cross}/cross-set"));
    wait_until("cross-site cookie seeded", || {
        source.view.title().as_deref() == Some("Cross seeded")
    });
    browser.navigate(&format!("{}/cross", fixture.base));
    wait_until("cross-site cookie available in normal mode", || {
        source.view.title().as_deref() == Some("Cross shared")
    });
    browser.settings.borrow_mut().strict_privacy = true;
    browser.send_native_privacy();
    wait_until("strict cookie preference accepted", || {
        browser.native_privacy_ready.get().is_some()
    });
    assert_eq!(browser.native_privacy_ready.get(), Some(true));
    browser.navigate(&format!("{}/cross-write", fixture.base));
    wait_until("third-party cookie sending denied", || {
        source.view.title().as_deref() == Some("Cross missing")
    });
    browser.navigate(&format!("{cross}/cross-read"));
    wait_until(
        "third-party cookie saving denied; first-party cookie kept",
        || source.view.title().as_deref() == Some("Cross shared"),
    );
    let strict = browser.add_tab(None, Some(&format!("{}/privacy", fixture.base)));
    let probe = std::cell::Cell::new(Instant::now());
    wait_until("strict WebRTC API policy in parent and child", || {
        if probe.get().elapsed() > Duration::from_secs(2) {
            strict.evaluate_javascript("document.title='RTC probe:'+typeof RTCPeerConnection+'/'+typeof document.querySelector('iframe').contentWindow.RTCPeerConnection");
            eprintln!("Strict frame probe: {:?}", strict.title());
            probe.set(Instant::now());
        }
        strict.title().as_deref() == Some("WebRTC disabled in both frames")
    });
    browser.close_tab(browser.active_tab().unwrap().id);
    browser.settings.borrow_mut().strict_privacy = false;
    browser.send_native_privacy();
    wait_until("normal cookie preference restored", || {
        browser.native_privacy_ready.get().is_some()
    });
    assert_eq!(browser.native_privacy_ready.get(), Some(true));
    let normal = browser.add_tab(None, Some(&format!("{}/privacy-normal", fixture.base)));
    wait_until("normal page restored", || {
        normal.title().as_deref() == Some("Paisley native fixture")
    });
    normal.evaluate_javascript(
        "document.title=typeof RTCPeerConnection==='function'?'WebRTC available':'WebRTC missing'",
    );
    wait_until("WebRTC available in a newly opened normal tab", || {
        normal.title().as_deref() == Some("WebRTC available")
    });
    browser.close_tab(browser.active_tab().unwrap().id);
    eprintln!(
        "CEF privacy: third-party cookie sending/saving blocked; WebRTC disabled in fresh parent/child frames and available again in normal mode"
    );
}

#[cfg(feature = "cef")]
fn script_dialog_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    fn widgets(root: &gtk::Widget) -> Vec<gtk::Widget> {
        let mut result = vec![root.clone()];
        let mut child = root.first_child();
        while let Some(widget) = child {
            result.extend(widgets(&widget));
            child = widget.next_sibling();
        }
        result
    }
    fn dialog(message: &str) -> Option<gtk::Window> {
        gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .find(|w| {
                w.is_visible()
                    && widgets(w.upcast_ref()).iter().any(|c| {
                        c.downcast_ref::<gtk::Label>()
                            .is_some_and(|l| l.text() == message)
                    })
            })
    }
    fn button(window: &gtk::Window, label: &str) -> gtk::Button {
        widgets(window.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|b| b.label().as_deref() == Some(label))
            .expect("dialog button")
    }
    let view = browser.add_tab(None, Some(&format!("{}/dialogs", fixture.base)));
    wait_until("dialog fixture loaded", || {
        view.title().as_deref() == Some("Paisley native fixture")
    });
    view.evaluate_javascript("alert('Paisley alert fixture');document.title='Alert returned'");
    wait_until("native alert presented in GTK", || {
        dialog("Paisley alert fixture").is_some()
    });
    let alert = dialog("Paisley alert fixture").unwrap();
    let weak = alert.downgrade();
    button(&alert, "OK").emit_clicked();
    drop(alert);
    wait_until("alert callback completed", || {
        view.title().as_deref() == Some("Alert returned")
    });
    wait_until("alert window released", || weak.upgrade().is_none());
    view.evaluate_javascript(
        "document.title=confirm('Paisley confirm fixture')?'Confirmed':'Cancelled'",
    );
    wait_until("native confirm presented in GTK", || {
        dialog("Paisley confirm fixture").is_some()
    });
    dialog("Paisley confirm fixture").unwrap().close();
    wait_until("confirm cancellation returned", || {
        view.title().as_deref() == Some("Cancelled")
    });
    view.evaluate_javascript("document.title='Prompt:'+prompt('Paisley prompt fixture','default')");
    wait_until("native prompt presented in GTK", || {
        dialog("Paisley prompt fixture").is_some()
    });
    let prompt = dialog("Paisley prompt fixture").unwrap();
    let entry = widgets(prompt.upcast_ref())
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Entry>().ok())
        .unwrap();
    assert_eq!(entry.text(), "default");
    entry.set_text("日本語 🦋");
    let weak = prompt.downgrade();
    button(&prompt, "OK").emit_clicked();
    drop(entry);
    drop(prompt);
    wait_until("Unicode prompt result returned", || {
        view.title().as_deref() == Some("Prompt:日本語 🦋")
    });
    wait_until("prompt window released", || weak.upgrade().is_none());
    view.evaluate_javascript("prompt('Paisley abandoned fixture','draft')");
    wait_until("pending prompt presented", || {
        dialog("Paisley abandoned fixture").is_some()
    });
    let weak = dialog("Paisley abandoned fixture").unwrap().downgrade();
    browser.close_tab(browser.active_tab().unwrap().id);
    wait_until("tab closure releases its prompt", || {
        weak.upgrade().is_none()
    });
    eprintln!("CEF dialogs: alert, confirm cancellation, Unicode prompt and owner cleanup passed");
}

#[cfg(feature = "cef")]
fn public_memory_lifecycle(browser: &Rc<Browser>) {
    let mut tabs = Vec::new();
    for (index, url) in [
        "https://github.com/",
        "https://mail.google.com/",
        "https://www.google.com/",
        "https://www.youtube.com/",
        "https://www.rust-lang.org/",
        "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        "https://docs.rs/",
        "https://example.com/",
    ]
    .into_iter()
    .enumerate()
    {
        let tab = if index == 0 {
            browser.active_tab().unwrap()
        } else {
            browser.add_tab(None, None);
            browser.active_tab().unwrap()
        };
        tab.committed.set(false);
        browser.load_tab(&tab, url);
        wait_until("public website navigation completed", || {
            tab.error.is_visible() || (tab.committed.get() && !tab.view.is_loading())
        });
        assert!(
            !tab.error.is_visible(),
            "public website failed: {url}: {}",
            tab.error.text()
        );
        eprintln!("PUBLIC_SITE {url} title={:?}", tab.view.title());
        tabs.push(tab);
        memory_stage(&format!("public sites loaded: {}", index + 1));
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    memory_stage("eight public sites before sleep");
    browser
        .residency_epoch
        .set(Instant::now() - Duration::from_secs(400));
    for tab in &tabs {
        tab.last_used.set(Instant::now() - Duration::from_secs(350));
    }
    *browser.memory.borrow_mut() = paisley::memory::sample(std::process::id());
    browser.reclaim_idle_tabs();
    let sleeping = tabs.iter().filter(|t| t.discarded.get()).count();
    assert!(sleeping > 0, "clean inactive public pages should sleep");
    assert!(
        tabs.last().unwrap().view.has_renderer(),
        "active public page protected"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    memory_stage("public sites after idle reclamation");
    for tab in tabs.iter().take(7) {
        browser.close_tab(tab.id);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    memory_stage("public sites returned to one tab");
    eprintln!("PUBLIC_SITE residency: {sleeping} inactive tabs slept; active page protected");
}

#[cfg(feature = "cef")]
fn auth_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    fn dialog() -> Option<gtk::Window> {
        gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .find(|w| w.is_visible() && w.title().as_deref() == Some("Authentication Required"))
    }
    browser.add_tab(None, Some(&format!("{}/auth-pending", fixture.base)));
    wait_until("native authentication prompt", || dialog().is_some());
    let weak = dialog().unwrap().downgrade();
    browser.close_tab(browser.active_tab().unwrap().id);
    wait_until("closed authentication owner releases prompt", || {
        weak.upgrade().is_none()
    });
    let view = browser.add_tab(None, Some(&format!("{}/auth/accepted", fixture.base)));
    wait_until("second native authentication prompt", || dialog().is_some());
    let window = dialog().unwrap();
    let weak = window.downgrade();
    let content = window.child().unwrap().downcast::<gtk::Box>().unwrap();
    let mut child = content.first_child();
    let mut entries = Vec::new();
    while let Some(widget) = child {
        if let Ok(entry) = widget.clone().downcast::<gtk::Entry>() {
            entries.push(entry);
        }
        child = widget.next_sibling();
    }
    assert_eq!(entries.len(), 2);
    entries[0].set_text("vox");
    entries[1].set_text("fixture");
    content
        .last_child()
        .unwrap()
        .last_child()
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap()
        .emit_clicked();
    drop(entries);
    drop(content);
    drop(window);
    wait_until("native HTTP credentials accepted", || {
        view.title().as_deref() == Some("Authentication accepted")
    });
    wait_until("authenticated prompt released", || weak.upgrade().is_none());
    browser.close_tab(browser.active_tab().unwrap().id);
    eprintln!("CEF authentication: HTTP Basic challenge, credentials and owner cleanup passed");
}

#[cfg(feature = "cef")]
fn file_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    let view = browser.add_tab(None, Some(&format!("{}/file", fixture.base)));
    wait_until("file input fixture loaded", || {
        view.title().as_deref() == Some("Paisley native fixture")
    });
    view.test_click(50.0, 50.0);
    wait_until("native file selection reached GTK", || {
        view.file_choice_pending()
    });
    eprintln!("FILE_CHOOSER_READY select tests/fixtures/downloads/sample.txt on Bot Screen");
    wait_until("selected file read through the page File API", || {
        view.title().as_deref() == Some("Uploaded exact fixture")
    });
    assert!(!view.file_choice_pending());
    view.test_click(50.0, 50.0);
    wait_until("second native file selection reached GTK", || {
        view.file_choice_pending()
    });
    browser.close_tab(browser.active_tab().unwrap().id);
    wait_until("closed file input owner cancels selection", || {
        !view.file_choice_pending()
    });
    eprintln!(
        "CEF file chooser: actual file input, selected text bytes and owner cancellation passed"
    );
}

#[cfg(feature = "cef")]
fn select_popup_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    let view = browser.add_tab(None, Some(&format!("{}/select", fixture.base)));
    wait_until("select fixture loaded", || {
        view.title().as_deref() == Some("Paisley native fixture")
    });
    wait_until("select page painted at its current size", || {
        view.input_surface_ready()
    });
    view.test_click(50.0, 40.0);
    wait_until("select menu actually painted", || {
        view.popup_has_painted_frame()
    });
    view.test_named_key("ArrowDown", 111);
    view.test_named_key("Enter", 36);
    wait_until("native select changed", || {
        view.title().as_deref() == Some("Selected:two")
    });
    wait_until("dismissed select releases popup texture", || {
        !view.popup_has_painted_frame()
    });
    for _ in 0..20 {
        view.test_click(50.0, 40.0);
        wait_until("reopened select painted", || view.popup_has_painted_frame());
        view.test_named_key("Escape", 9);
        wait_until("select dismissed", || !view.popup_has_painted_frame());
    }
    browser.close_tab(browser.active_tab().unwrap().id);
    eprintln!(
        "CEF select popup: painted, Unicode option selected and 20 reopen/dismiss cycles passed"
    );
}

#[cfg(feature = "cef")]
fn file_cancellation_lifecycle(browser: &Rc<Browser>, fixture: &Fixture) {
    for _ in 0..12 {
        let view = browser.add_tab(None, Some(&format!("{}/file", fixture.base)));
        wait_until("file cancellation fixture painted", || {
            view.input_surface_ready() && view.title().as_deref() == Some("Paisley native fixture")
        });
        view.test_click(50.0, 50.0);
        wait_until("file chooser owned by tab", || view.file_choice_pending());
        let window = view.file_choice_window().unwrap();
        browser.close_tab(browser.active_tab().unwrap().id);
        wait_until("file chooser window released", || {
            !view.file_choice_pending() && window.upgrade().is_none()
        });
    }
    eprintln!("CEF file chooser: 12 immediate owner-close cycles released their windows");
}
