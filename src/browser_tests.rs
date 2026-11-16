use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

mod benchmarks {
    include!("browser_benchmarks.rs");
}

mod restart_diagnostics {
    include!("restart_diagnostics.rs");
}

struct LocalSite {
    base: String,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    trackers: Arc<std::sync::atomic::AtomicUsize>,
}

impl LocalSite {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::clone(&stop);
        let trackers = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&trackers);
        let worker = std::thread::spawn(move || {
            // Test-only pending resources: close them when the owned server stops.
            let mut held_resources = Vec::new();
            while !shutdown.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut buffer = [0; 4096];
                        let count = stream.read(&mut buffer).unwrap_or(0);
                        let request = String::from_utf8_lossy(&buffer[..count]);
                        let target = request.split_whitespace().nth(1).unwrap_or("/");
                        let path = target.split('?').next().unwrap_or("/");
                        let response = match path {
                            "/abort" => continue, // Fail before any document can commit.
                            "/held-image" => {
                                if held_resources.len() < 16 { held_resources.push(stream); }
                                continue;
                            }
                            "/unfinished" => {
                                let body = "<!doctype html><title>Unfinished</title><h1>Visible committed document</h1><img src='/held-image'>";
                                format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                            }
                            "/blocking" => {
                                let body = "<!doctype html><title>Blocking fixture</title><script src='/tracker.js'></script><script>window.allowedScript=true</script>";
                                format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                            }
                            "/tracker.js" => {
                                counted.fetch_add(1, Ordering::Relaxed);
                                "HTTP/1.1 200 OK\r\nCache-Control: no-store\r\nContent-Type: text/javascript\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                            }
                            "/embedded" => {
                                let body = include_str!("../tests/fixtures/embedded-content.html");
                                format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                            }
                            "/redirect" => "HTTP/1.1 302 Found\r\nLocation: /two\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
                            "/download" => "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=test.bin\r\nContent-Length: 4\r\nConnection: close\r\n\r\ntest".to_owned(),
                            "/large-download" => {
                                let body = "x".repeat(4 * 1024 * 1024);
                                format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=large.bin\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                            }
                            "/broken-download" => "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=broken.bin\r\nContent-Length: 16\r\nConnection: close\r\n\r\npart".to_owned(),
                            "/pending-download" => {
                                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=pending.bin\r\nContent-Length: 65536\r\nConnection: close\r\n\r\n");
                                let _ = stream.write_all(&[b'x'; 4096]); // Enough bytes for native MIME sniffing; remainder deliberately pending.
                                if held_resources.len() < 16 { held_resources.push(stream); }
                                continue;
                            },
                            _ => {
                                let title = if path.starts_with("/two") { "Two" } else { "One" };
                                let body = format!("<!doctype html><title>{title}</title><h1>{title}</h1><a href='/two'>Next</a>");
                                format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                            }
                        };
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("local server failed: {error}"),
                }
            }
        });
        Self {
            base,
            stop,
            worker: Some(worker),
            trackers,
        }
    }
}

impl Drop for LocalSite {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn defer_engine_fixture_cleanup_until_process_exit(root: &std::path::Path) {
    // Releasing a NetworkSession wrapper is not proof that the shared native
    // NetworkProcess has discarded every cache watcher. Deleting its storage
    // here repeatedly triggered a NetworkCache callback crash in the fixture.
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    assert!(
        root.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("paisley-")
    );
    eprintln!(
        "owned engine fixture retained: {}; isolated runner removes it at namespace exit; direct runs require cleanup after process exit",
        root.display()
    );
}

#[test]
fn engine_fixture_teardown_keeps_owned_storage_until_process_exit() {
    let root =
        std::env::temp_dir().join(format!("paisley-teardown-lifetime-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("owned-fixture"),
        b"retain until native process exit",
    )
    .unwrap();
    defer_engine_fixture_cleanup_until_process_exit(&root);
    let retained = std::fs::read(root.join("owned-fixture"));
    // This unit fixture never creates WebKit state, so immediate cleanup is safe.
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    assert_eq!(retained.unwrap(), b"retain until native process exit");
}

fn isolated_resource_snapshot() {
    if std::env::var_os("PAISLEY_DOWNLOAD_CAPACITY_FIXTURE").is_none() {
        return;
    }
    let output = std::process::Command::new("/usr/bin/python3").args(["-I", "-c", r#"
import os, pathlib, resource
print('owned resource snapshot: RLIMIT_NOFILE=', resource.getrlimit(resource.RLIMIT_NOFILE))
for name in ('/', '/tmp', '/dev/shm', '/home/agent', '/tmp/paisley-download-capacity'):
    try:
        value = os.statvfs(name)
        print('owned filesystem:', name, 'available_bytes=', value.f_bavail * value.f_frsize, 'total_bytes=', value.f_blocks * value.f_frsize, 'free_inodes=', value.f_favail)
    except OSError:
        print('owned filesystem unavailable:', name)
# Do not enumerate an ordinary host's processes if this helper is invoked
# outside our isolated test runner. Only report per-process descriptors in
# the private bwrap PID namespace.
if pathlib.Path('/proc/1/comm').read_text().strip() == 'bwrap':
    counts = []
    for path in pathlib.Path('/proc').iterdir():
        if not path.name.isdigit():
            continue
        try:
            fds = len(list((path / 'fd').iterdir()))
            threads = next(line.split(':', 1)[1].strip() for line in (path / 'status').read_text().splitlines() if line.startswith('Threads:'))
            counts.append((fds, path.name, (path / 'comm').read_text().strip(), threads))
        except (OSError, StopIteration):
            pass
    print('owned processes by descriptor count:', sorted(counts, reverse=True)[:8])
"#]).output();
    if let Ok(output) = output {
        eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    }
}

fn wait_until(description: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        while gtk::glib::MainContext::default().pending() {
            gtk::glib::MainContext::default().iteration(false);
        }
        if condition() {
            return;
        }
        if Instant::now() >= deadline {
            isolated_resource_snapshot();
            panic!("timed out waiting for {description}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn capture_window(window: &gtk::ApplicationWindow) {
    capture_native_window(window.upcast_ref(), "PAISLEY_SCREENSHOT");
}

fn capture_native_window(window: &gtk::Window, variable: &str) {
    let Ok(path) = std::env::var(variable) else {
        return;
    };
    // present() maps asynchronously; rendering a zero-sized node can abort GSK.
    wait_until("allocated screenshot window", || {
        window.is_mapped() && window.width() > 0 && window.height() > 0
    });
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let mut node = None;
    wait_until("window snapshot", || {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            &snapshot,
            f64::from(window.width()),
            f64::from(window.height()),
        );
        node = snapshot.to_node();
        node.is_some()
    });
    window
        .renderer()
        .expect("window renderer")
        .render_texture(node.unwrap(), None)
        .save_to_png(path)
        .expect("save browser screenshot");
}

fn javascript(view: &webkit::WebView, script: &str) -> String {
    let result = Rc::new(RefCell::new(None));
    let output = Rc::clone(&result);
    view.evaluate_javascript(
        script,
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |value| {
            *output.borrow_mut() = Some(value.expect("fixture JavaScript").to_str().to_string());
        },
    );
    wait_until("JavaScript result", || result.borrow().is_some());
    result.borrow_mut().take().unwrap()
}

fn check_duckduckgo_promotions(view: &webkit::WebView) {
    for (index, base, expected) in [
        (
            0,
            "https://duckduckgo.com/",
            "false,false,true,true,true,false,false",
        ),
        (
            1,
            "https://start.duckduckgo.com/",
            "false,false,true,true,true,false,false",
        ),
        (
            2,
            "https://example.org/",
            "true,true,true,true,true,true,true",
        ),
        (
            3,
            "https://duckduckgo.com.example.org/",
            "true,true,true,true,true,true,true",
        ),
    ] {
        let title = format!("Promotion fixture {index}");
        let html = format!(
            r#"<!doctype html><title>{title}</title>
            <form id="search"><input name="q"><button>Search</button></form>
            <section id="promotion" class="desktop-homepage-module-scss-module__fixture__ctaSection">
              <ul id="cards" data-testid="cta-cards-sad">
                <li>Make DuckDuckGo your default search engine.</li>
                <li>BEST PRIVACY Get our free browser for even more privacy.</li>
              </ul>
            </section>
            <section id="download-promotion" data-testid="download-panel">Get our free browser today!</section>
            <a id="result" href="https://example.com/">A search result about browsers</a>
            <nav id="menu"><a id="manual-download" href="/app">About Our Browser</a></nav>"#
        );
        // Local bytes with a synthetic base URI exercise native domain scoping
        // without downloading or depending on the public website.
        view.load_html(&html, Some(base));
        wait_until("promotion fixture", || {
            view.title().as_deref() == Some(title.as_str()) && !view.is_loading()
        });
        let visibility = javascript(
            view,
            r#"
            document.body.insertAdjacentHTML('beforeend',
              '<ul id="dynamic-promotion" data-testid="cta-cards-browser"><li>Get our free browser</li></ul>');
            ['promotion', 'cards', 'search', 'result', 'manual-download', 'dynamic-promotion', 'download-promotion']
              .map(id => document.getElementById(id).getClientRects().length > 0).join(',')
        "#,
        );
        assert_eq!(visibility, expected, "promotion filtering scope for {base}");
    }
}

fn check_embedded_content(browser: &Browser, site: &LocalSite) {
    let tab = browser.active_tab().unwrap();
    let uri = format!("{}/embedded", site.base);
    browser.navigate(&uri);
    wait_until("embedded-content page", || {
        tab.error.is_visible()
            || (tab.view.title().as_deref() == Some("Embedded content fixture")
                && !tab.view.is_loading())
    });
    assert!(
        !tab.error.is_visible(),
        "embedded content was rejected: {}",
        tab.error.text()
    );
    wait_until("embedded documents executing", || {
        tab.error.is_visible()
            || javascript(&tab.view, "Array.from(window.framesReady).sort().join(',')")
                == "blank,blob,data,srcdoc"
    });
    assert!(
        !tab.error.is_visible(),
        "embedded content failed: {}",
        tab.error.text()
    );
    assert_eq!(
        javascript(&tab.view, "Array.from(window.framesReady).sort().join(',')"),
        "blank,blob,data,srcdoc"
    );
    assert_eq!(tab.view.uri().as_deref(), Some(uri.as_str()));
    assert_eq!(browser.address.text().as_str(), uri);
}

#[test]
#[ignore = "requires a graphical session and installed GTK4/WebKitGTK libraries"]
fn browser_smoke() {
    gtk::init().expect("GTK initialization failed; run in a graphical session");
    let app = gtk::Application::builder()
        .application_id("site.blig.Paisley.Tests")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    if let Some(output) = std::env::var_os("PAISLEY_BENCHMARK_OUTPUT") {
        let site = LocalSite::start();
        benchmarks::run(&app, &site, std::path::Path::new(&output));
        return;
    }
    if let Ok(stage) = std::env::var("PAISLEY_RESTART_CHILD") {
        check_restart_child(&app, &stage);
        return;
    }
    let site = LocalSite::start();
    if std::env::var_os("PAISLEY_TAB_LIFETIME").is_some() {
        check_closed_tab_lifetime(&app, &site);
        return;
    }
    if std::env::var_os("PAISLEY_BLOCKING_BOUNDARIES").is_some() {
        check_blocking_navigation_boundaries(&app, &site);
        return;
    }
    if std::env::var_os("PAISLEY_INSPECTOR_LIFECYCLE").is_some()
        || std::env::var_os("PAISLEY_INSPECTOR_OPEN_CLOSE").is_some()
    {
        if std::env::var_os("PAISLEY_INSPECTOR_PREFACE").is_some()
            || std::env::var_os("PAISLEY_INSPECTOR_OPEN_CLOSE").is_some()
        {
            check_settings_preferences(&app, &site);
            check_non_restorable_sessions(&app, &site);
        }
        if std::env::var_os("PAISLEY_INSPECTOR_OPEN_CLOSE").is_some() {
            check_inspector_open_close(&app, &site);
        } else {
            check_inspector_lifecycle(&app, &site);
        }
        return;
    }
    if std::env::var_os("PAISLEY_RESTART_PROCESSES").is_some() {
        check_process_restarts(&site);
        return;
    }
    if std::env::var_os("PAISLEY_RESTART_STRESS").is_some() {
        let _monitor = restart_diagnostics::NetworkExitMonitor::start();
        for cycle in 1..=8 {
            let processes = std::fs::read_dir("/proc")
                .unwrap()
                .flatten()
                .filter(|entry| entry.file_name().to_string_lossy().parse::<u32>().is_ok())
                .count();
            eprintln!(
                "restart stress cycle {cycle}/8: visible_processes={processes} app_windows={} gtk_windows={}",
                app.windows().len(),
                gtk::Window::list_toplevels().len()
            );
            isolated_resource_snapshot();
            check_persistent_profiles(&app, &site, cycle);
        }
        return;
    }
    check_settings_preferences(&app, &site);
    check_non_restorable_sessions(&app, &site);
    check_inspector_lifecycle(&app, &site);
    check_bookmark_transfer_ui(&app, &site);
    let browser = Browser::new(&app);
    // This fixture drives actions programmatically. Ignore unrelated desktop
    // key events so a real desktop session cannot alter an address draft.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(|_, _, _, _| gtk::glib::Propagation::Stop);
    browser.window.add_controller(keys);
    browser.present();
    wait_until("initial blocker ready", || browser.blocker.ready.get());

    assert_eq!(browser.notebook.n_pages(), 1);
    let first = browser.active_tab().unwrap();
    // Assert the requested home destination without depending on the public
    // website's availability; subsequent navigations use the local fixture.
    assert_eq!(first.view.uri().as_deref(), Some("https://duckduckgo.com/"));
    assert_eq!(browser.address.text().as_str(), "https://duckduckgo.com/");
    assert!(browser.session.is_ephemeral());
    assert!(
        webkit::prelude::WebViewExt::settings(&first.view)
            .unwrap()
            .enables_developer_extras()
    );
    assert!(browser.session.is_itp_enabled());

    browser.navigate(&format!("{}/one", site.base));
    assert_eq!(
        browser.address.text().as_str(),
        format!("{}/one", site.base),
        "explicit navigation must update the address even before the WebView takes focus"
    );
    wait_until("first page", || {
        first.view.title().as_deref() == Some("One") && !first.view.is_loading()
    });
    assert_eq!(
        browser.address.text().as_str(),
        format!("{}/one", site.base)
    );
    assert!(!browser.back.is_sensitive());

    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.new-tab", None).unwrap();
    assert_eq!(browser.notebook.n_pages(), 2);
    let second = browser.active_tab().unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(
        second.view.uri().as_deref(),
        Some("https://duckduckgo.com/")
    );
    browser.navigate(&format!("{}/two", site.base));
    wait_until("second page", || {
        second.view.title().as_deref() == Some("Two") && !second.view.is_loading()
    });
    wait_until("owned-process RAM sample", || {
        browser.memory.borrow().is_some()
    });
    assert!(browser.window.title().unwrap().contains("Paisley ≈"));
    assert!(browser.status.text().contains("Paisley ≈"));
    browser.settings.borrow_mut().show_memory = false;
    browser.apply_settings();
    assert!(!browser.window.title().unwrap().contains("Paisley ≈"));
    browser.settings.borrow_mut().show_memory = true;
    browser.apply_settings();
    capture_window(&browser.window);
    assert!(browser.window.title().unwrap().starts_with("Two — Paisley"));
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.inspector", None).unwrap();
    let inspector = second.view.inspector().expect("native Web Inspector");
    let inspector_frontend = mapped_inspector(&inspector);
    // inspected-uri is optional on this engine; verify active-view ownership
    // through the two distinct Inspector objects rather than require a URI.
    assert!(
        !first
            .view
            .inspector()
            .unwrap()
            .web_view()
            .is_some_and(|view| view.is_visible())
    );
    inspector.close();
    wait_until("active tab Inspector closed", || {
        !inspector_frontend.is_mapped()
    });
    assert!(!second.view.can_go_back());

    first.view.load_uri(&format!("{}/redirect", site.base));
    wait_until("background redirect", || {
        first.view.title().as_deref() == Some("Two") && !first.view.is_loading()
    });
    assert_eq!(
        browser.address.text().as_str(),
        format!("{}/two", site.base)
    );
    assert!(
        !browser.back.is_sensitive(),
        "background history overwrote active controls"
    );

    browser.notebook.set_current_page(Some(0));
    assert_eq!(browser.active_tab().unwrap().id, first.id);
    assert!(browser.back.is_sensitive());
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.back", None).unwrap();
    wait_until("back navigation", || {
        first.view.title().as_deref() == Some("One") && !first.view.is_loading()
    });
    assert!(browser.forward.is_sensitive());
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.forward", None).unwrap();
    wait_until("forward navigation", || {
        first.view.title().as_deref() == Some("Two") && !first.view.is_loading()
    });

    browser.focus_address();
    browser.address.set_text("unsubmitted.example");
    assert_eq!(
        browser.address.text().as_str(),
        "unsubmitted.example",
        "draft immediately after set_text"
    );
    first.view.evaluate_javascript(
        "document.title = 'Updated title'",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |result| {
            result.expect("local title update");
        },
    );
    wait_until("title update during address edit", || {
        first.view.title().as_deref() == Some("Updated title")
    });
    assert_eq!(
        browser.address.text().as_str(),
        "unsubmitted.example",
        "page notification discarded an address draft"
    );

    browser.notebook.reorder_child(&second.page, Some(0));
    browser.close_tab(second.id);
    assert_eq!(browser.notebook.n_pages(), 1);
    assert_eq!(browser.active_tab().unwrap().id, first.id);
    assert_eq!(
        browser.address.text().as_str(),
        "unsubmitted.example",
        "closing a background tab discarded an address draft"
    );

    browser.navigate("javascript:alert(1)");
    assert!(first.error.is_visible());
    assert_eq!(
        first.view.uri().as_deref(),
        Some(format!("{}/two", site.base).as_str())
    );

    let download_dir =
        std::env::temp_dir().join(format!("paisley-download-ui-{}", std::process::id()));
    assert!(!download_dir.exists());
    std::fs::create_dir(&download_dir).unwrap();
    *browser.downloads.borrow_mut() =
        crate::downloads::Downloads::with_directory(&browser.window, download_dir.clone());
    browser.navigate(&format!("{}/download", site.base));
    wait_until("attachment saved", || {
        !browser.downloads.borrow().completed_paths().is_empty()
            || !browser.downloads.borrow().errors().is_empty()
    });
    assert!(
        browser.downloads.borrow().errors().is_empty(),
        "{:?}",
        browser.downloads.borrow().errors()
    );
    let paths = browser.downloads.borrow().completed_paths();
    assert_eq!(paths.len(), 1);
    assert_eq!(std::fs::read(&paths[0]).unwrap(), b"test");
    assert_eq!(browser.downloads.borrow().active_count(), 0);
    let checks = Rc::new(RefCell::new(Vec::new()));
    let session = webkit::NetworkSession::new_ephemeral();
    let output = Rc::clone(&checks);
    let parent = browser.window.clone();
    session.connect_download_started(move |_, download| {
        let output = Rc::clone(&output);
        let parent = parent.clone();
        download.connect_decide_destination(move |download, _| {
            output
                .borrow_mut()
                .push(crate::downloads::PendingChoice::new(&parent, download));
            true
        });
    });
    for count in 1..=3 {
        session
            .download_uri(&format!("{}/download", site.base))
            .unwrap();
        wait_until("pending download chooser", || {
            checks.borrow().len() >= count
        });
        if count == 1 {
            checks.borrow()[count - 1].cancel_row();
        } else if count == 2 {
            checks.borrow()[count - 1].interrupt_transfer();
        } else {
            // Simulate the selected parent disappearing before native writing.
            checks.borrow()[count - 1]
                .accept_late_path(download_dir.join("missing-parent").join("file.bin"));
        }
        let expected = if count == 1 { "Cancelled" } else { "Failed" };
        wait_until("terminal state while chooser is open", || {
            checks.borrow()[count - 1].status() == expected
        });
        let records = checks.borrow()[count - 1].records();
        assert_eq!(records.records.len(), 1);
        assert_eq!(
            records.records[0].outcome,
            if count == 1 {
                paisley::download_history::Outcome::Cancelled
            } else {
                paisley::download_history::Outcome::Failed
            }
        );
        if count == 3 {
            assert!(!download_dir.join("missing-parent/file.bin").exists());
        }
        checks.borrow()[count - 1].accept_late_path(download_dir.join(format!("late-{count}")));
        assert_eq!(
            checks.borrow()[count - 1].status(),
            expected,
            "late chooser callback must not resurrect a terminal download"
        );
    }
    checks.borrow_mut().clear();
    std::fs::remove_dir_all(download_dir).unwrap();

    // Use an owned server that closes the connection before a response. Unsafe
    // port 1 can be converted to about:blank by this WebKit version instead.
    browser.navigate(&format!("{}/abort", site.base));
    wait_until("network failure", || {
        first.error.is_visible() && first.error.text().contains("Could not load")
    });
    assert!(first.error.text().contains("127.0.0.1"));

    browser.navigate(&format!("{}/one", site.base));
    wait_until("recovery after failure", || {
        first.view.title().as_deref() == Some("One") && !first.view.is_loading()
    });
    assert!(!first.error.is_visible());

    let closing = browser.add_tab(None, Some(&format!("{}/two", site.base)));
    let closing_id = browser.active_tab().unwrap().id;
    let weak_view = closing.downgrade();
    drop(closing);
    browser.close_tab(closing_id);
    wait_until("closed WebView released", || weak_view.upgrade().is_none());
    assert_eq!(browser.active_tab().unwrap().id, first.id);

    browser.close_tab(first.id);
    assert_eq!(
        browser.notebook.n_pages(),
        1,
        "closing last tab must create a welcome tab"
    );
    let replacement = browser.active_tab().unwrap();
    assert_ne!(replacement.id, first.id);
    assert_eq!(
        replacement.view.uri().as_deref(),
        Some("https://duckduckgo.com/")
    );
    assert_eq!(browser.address.text().as_str(), "https://duckduckgo.com/");
    assert!(!browser.back.is_sensitive());

    browser.navigate(&format!("{}/one", site.base));
    wait_until("page before Home", || {
        replacement.view.title().as_deref() == Some("One") && !replacement.view.is_loading()
    });
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.home", None).unwrap();
    assert_eq!(
        replacement.view.uri().as_deref(),
        Some("https://duckduckgo.com/")
    );
    if std::env::var_os("PAISLEY_LIVE_DUCKDUCKGO").is_some() {
        wait_until("live DuckDuckGo homepage", || {
            replacement
                .view
                .title()
                .is_some_and(|title| title.contains("DuckDuckGo"))
                && !replacement.view.is_loading()
        });
        let visible = javascript(
            &replacement.view,
            r#"
            (() => {
                const input = document.querySelector('input[name="q"], textarea[name="q"]');
                const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
                let node, hasVisiblePromo = false;
                while ((node = walker.nextNode())) {
                    const parent = node.parentElement;
                    if (parent && !parent.closest('script, style, noscript') &&
                        /make duckduckgo your default search engine|get our free browser/i.test(node.textContent) &&
                        parent.getClientRects().length > 0) hasVisiblePromo = true;
                }
                return [Boolean(input && input.getClientRects().length), hasVisiblePromo].join(',');
            })()
        "#,
        );
        capture_window(&browser.window);
        if visible != "true,false" {
            eprintln!(
                "Live page diagnostics: {}",
                javascript(
                    &replacement.view,
                    r#"
                (() => {
                    const promos = [];
                    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
                    let node;
                    while ((node = walker.nextNode())) {
                        if (!node.parentElement.closest('script,style,noscript') &&
                            /make duckduckgo your default search engine|get our free browser/i.test(node.textContent)) {
                            let parent = node.parentElement, ancestors = [];
                            for (let i=0; parent && i<5; i++, parent=parent.parentElement)
                                ancestors.push({tag:parent.tagName, id:parent.id, cls:parent.className, testid:parent.getAttribute('data-testid')});
                            promos.push({text:node.textContent,ancestors});
                        }
                    }
                    return JSON.stringify({url:location.href,fields:[...document.querySelectorAll('input,textarea')].map(e=>e.outerHTML),promos});
                })()
            "#
                )
            );
        }
        assert_eq!(
            visible, "true,false",
            "live search form must remain visible without install promotions"
        );
    }
    check_duckduckgo_promotions(&replacement.view);
    check_embedded_content(&browser, &site);
    if std::env::var_os("PAISLEY_LIVE_DUCKAI").is_some() {
        browser.navigate(
            "https://duck.ai/chat?ia=chat&duckai=1&home=1&prompt=1&origin=funnel_home_website&t=h_",
        );
        wait_until("live Duck.ai interface", || {
            replacement.error.is_visible()
                || (!replacement.view.is_loading()
                    && javascript(
                        &replacement.view,
                        r#"(() => {
                    const visible = element => {
                        if (!element || element.disabled || !element.getClientRects().length) return false;
                        for (let parent = element; parent; parent = parent.parentElement) {
                            const style = getComputedStyle(parent);
                            if (style.visibility === 'hidden' || Number(style.opacity) < 0.95) return false;
                        }
                        const rect = element.getBoundingClientRect();
                        const target = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
                        return target && (target === element || element.contains(target));
                    };
                    return Array.from(document.querySelectorAll('textarea, [contenteditable="true"]')).some(visible) ||
                        Array.from(document.querySelectorAll('button')).some(button =>
                            visible(button) && /agree|continue|get started/i.test(button.textContent));
                })()"#,
                    ) == "true")
        });
        capture_window(&browser.window);
        assert!(
            !replacement.error.is_visible(),
            "Duck.ai failed: {}",
            replacement.error.text()
        );
    }
    browser.window.close();
    check_native_request_blocking(&app, &site);
    check_persistent_profiles(&app, &site, 0);
    check_process_restarts(&site);
    check_sensitive_permission_denial(&app, &site);
    check_browser_library(&app, &site);
    check_download_records(&app, &site);
    crate::automation_runtime::check_native_grants(&app, capture_native_window);
}

fn bookmark_confirmation(parent: &gtk::Window) -> gtk::Window {
    let mut dialog = None;
    wait_until("bookmark transfer confirmation", || {
        dialog = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .find(|w| w.is_visible() && w.transient_for().as_ref() == Some(parent));
        dialog.is_some()
    });
    dialog.unwrap()
}

fn bookmark_decide(dialog: &gtk::Window, choice: &str) {
    library_widgets(dialog.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some(choice))
        .unwrap()
        .emit_clicked();
    wait_until("bookmark transfer decision completed", || {
        !dialog.is_visible()
    });
}

fn check_bookmark_transfer_ui(app: &gtk::Application, site: &LocalSite) {
    for private in [false, true] {
        let base = std::env::temp_dir().join(format!(
            "paisley-bookmark-ui-{}-{private}",
            std::process::id()
        ));
        std::fs::create_dir(&base).unwrap();
        let mut profile = paisley::profile::Profile::open_at(&base, "transfer", private).unwrap();
        profile.settings.homepage = format!("{}/one", site.base);
        profile.settings.restore_session = false;
        let browser = Browser::with_profile(app, profile).unwrap();
        browser.present();
        let view = browser.active_tab().unwrap().view;
        wait_until("bookmark fixture loaded", || {
            !view.is_loading() && view.title().as_deref() == Some("One")
        });
        browser
            .library
            .borrow_mut()
            .bookmark("https://example.test/existing", "Keep", 7)
            .unwrap();
        browser.dirty.set(true);
        let editor = browser.show_library(true);
        let manager: gtk::Window = browser.window.clone().upcast();
        wait_until("bookmark sheet mapped", || {
            library_widgets(manager.upcast_ref()).iter().any(|w| {
                w.downcast_ref::<gtk::SearchEntry>()
                    .is_some_and(|w| w.is_mapped())
            })
        });
        for label in ["Import…", "Export…"] {
            assert!(
                library_widgets(manager.upcast_ref()).iter().any(|w| w
                    .downcast_ref::<gtk::Button>()
                    .is_some_and(|button| button.label().as_deref() == Some(label))),
                "native bookmark transfer control missing: {label}"
            );
        }
        let input = base.join("incoming.json");
        let valid = br#"{"version":1,"bookmarks":[{"uri":"https://example.test/existing","title":"Do not replace"},{"uri":"https://example.test/new","title":"<b>New</b>"}]}"#;
        std::fs::write(&input, valid).unwrap();
        let before = browser.library.borrow().clone();
        for confirm in [false, true] {
            editor.import_file(&input).unwrap();
            assert_eq!(
                *browser.library.borrow(),
                before,
                "selection must not import before consent"
            );
            let dialog = bookmark_confirmation(&manager);
            if confirm {
                // A bookmark added while consent is pending must not be lost.
                browser
                    .library
                    .borrow_mut()
                    .bookmark("https://late.test/", "Late", 8)
                    .unwrap();
                // Approval applies to the already validated snapshot, not a later
                // replacement of the selected file.
                std::fs::write(&input, br#"{"version":999,"bookmarks":[]}"#).unwrap();
            }
            bookmark_decide(&dialog, if confirm { "Import" } else { "Cancel" });
            if !confirm {
                assert_eq!(*browser.library.borrow(), before);
            }
        }
        assert_eq!(browser.library.borrow().history, before.history);
        assert_eq!(browser.library.borrow().bookmarks.len(), 3);
        assert_eq!(browser.library.borrow().bookmarks[1].title, "Keep");
        assert_eq!(
            view.uri().as_deref(),
            Some(format!("{}/one", site.base).as_str()),
            "import must never navigate"
        );
        assert!(
            library_widgets(manager.upcast_ref()).iter().any(|w| w
                .downcast_ref::<gtk::Label>()
                .is_some_and(|label| label.text() == "<b>New</b>" && !label.uses_markup())),
            "imported title must be rendered as literal text"
        );
        if !private {
            capture_native_window(&manager, "PAISLEY_BOOKMARKS_SCREENSHOT");
        }
        let imported = browser.library.borrow().clone();
        assert!(
            editor.import_file(&input).is_err(),
            "future input must be rejected"
        );
        assert_eq!(*browser.library.borrow(), imported);
        let output = base.join("export.json");
        editor.export_file(&output).unwrap();
        let exported = std::fs::read(&output).unwrap();
        assert_eq!(
            paisley::bookmark_transfer::parse_import(&exported)
                .unwrap()
                .count(),
            3
        );
        assert!(!String::from_utf8_lossy(&exported).contains("visited"));
        assert!(editor.export_file(&output).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), exported);

        library_widgets(manager.upcast_ref())
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|button| button.label().as_deref() == Some("Export…"))
            .unwrap()
            .emit_clicked();
        let dialog = bookmark_confirmation(&manager);
        assert!(library_widgets(dialog.upcast_ref()).iter().any(|w| {
            w.downcast_ref::<gtk::Label>()
                .is_some_and(|label| label.text().contains("survives closing"))
        }));
        if !private {
            capture_native_window(&dialog, "PAISLEY_BOOKMARK_EXPORT_SCREENSHOT");
        }
        bookmark_decide(&dialog, "Cancel");
        assert_eq!(*browser.library.borrow(), imported);

        let capacity = base.join("capacity.json");
        std::fs::write(
            &capacity,
            br#"{"version":1,"bookmarks":[{"uri":"https://capacity.test/","title":"New"}]}"#,
        )
        .unwrap();
        editor.import_file(&capacity).unwrap();
        let dialog = bookmark_confirmation(&manager);
        for n in 0..paisley::library::MAX_BOOKMARKS - imported.bookmarks.len() {
            browser
                .library
                .borrow_mut()
                .bookmark(&format!("https://fill.test/{n}"), "Filler", 0)
                .unwrap();
        }
        let full = browser.library.borrow().clone();
        bookmark_decide(&dialog, "Import");
        assert_eq!(
            *browser.library.borrow(),
            full,
            "confirmation-time capacity failure must be atomic"
        );
        *browser.library.borrow_mut() = imported.clone();
        browser.dirty.set(true);

        std::fs::write(&input, valid).unwrap();
        editor.import_file(&input).unwrap();
        let dialog = bookmark_confirmation(&manager);
        browser.menu.dismiss();
        // A held callback/controller must not make a closed manager writable.
        assert!(editor.import_file(&input).is_err());
        assert!(editor.export_file(&base.join("late-export.json")).is_err());
        wait_until("closed parent dismisses consent", || !dialog.is_visible());
        assert_eq!(*browser.library.borrow(), imported);
        let parent_editor = browser.show_library(true);
        let parent_manager: gtk::Window = browser.window.clone().upcast();
        wait_until("reopened bookmark sheet mapped", || {
            library_widgets(parent_manager.upcast_ref())
                .iter()
                .any(|w| {
                    w.downcast_ref::<gtk::SearchEntry>()
                        .is_some_and(|w| w.is_mapped())
                })
        });
        // Minimizing is not closing the sheet: its editor must still work
        // after restoring the owner, despite the new unmap cancellation hook.
        browser.window.minimize();
        let minimized_at = Instant::now();
        wait_until("allow window-manager minimize", || {
            minimized_at.elapsed() >= Duration::from_millis(250)
        });
        browser.window.unminimize();
        browser.present();
        wait_until("restored bookmark sheet mapped", || {
            library_widgets(parent_manager.upcast_ref())
                .iter()
                .any(|w| {
                    w.downcast_ref::<gtk::SearchEntry>()
                        .is_some_and(|w| w.is_mapped())
                })
        });
        parent_editor.import_file(&input).unwrap();
        let pending = bookmark_confirmation(&parent_manager);
        browser.window.close();
        wait_until("browser closure dismisses bookmark consent", || {
            !pending.is_visible()
        });
        assert!(
            parent_editor
                .export_file(&base.join("closed-browser.json"))
                .is_err()
        );
        assert_eq!(*browser.library.borrow(), imported);
        drop(view);
        drop(browser);
        if private {
            assert!(
                !base.join("transfer").exists(),
                "private import must not write profile state"
            );
        } else {
            let profile = paisley::profile::Profile::open_at(&base, "transfer", false).unwrap();
            assert_eq!(profile.load_library().unwrap(), imported);
        }
        assert_eq!(
            std::fs::read(&output).unwrap(),
            exported,
            "explicit private exports survive closing"
        );
        defer_engine_fixture_cleanup_until_process_exit(&base);
    }
}

fn check_settings_preferences(app: &gtk::Application, site: &LocalSite) {
    let parent = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Owned settings fixture")
        .build();
    parent.present();
    let current = paisley::profile::Settings {
        homepage: format!("{}/one", site.base),
        remember_history: false,
        ..Default::default()
    };
    let saved = Rc::new(RefCell::new(Vec::new()));
    let result = Rc::clone(&saved);
    let editor = crate::settings_ui::show(&parent, current.clone(), move |settings| {
        result.borrow_mut().push(settings);
        Ok(())
    });
    let window = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .find(|window| {
            window.title().as_deref() == Some("Paisley settings")
                && window.transient_for().as_ref() == Some(parent.upcast_ref())
        })
        .unwrap();
    // GtkExpander does not parent its collapsed content into the widget tree.
    // Follow the user action before inspecting the advanced controls.
    library_widgets(window.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Expander>().ok())
        .find(|expander| expander.label().as_deref() == Some("Advanced"))
        .expect("Advanced settings section")
        .set_expanded(true);
    for label in ["Import preferences…", "Export preferences…", "Reset draft…"] {
        assert!(
            library_widgets(window.upcast_ref())
                .iter()
                .any(|widget| widget
                    .downcast_ref::<gtk::Button>()
                    .is_some_and(|button| button.label().as_deref() == Some(label))),
            "missing native settings control: {label}"
        );
    }
    assert!(
        saved.borrow().is_empty(),
        "opening settings must not save anything"
    );
    let base = std::env::temp_dir().join(format!("paisley-settings-ui-{}", std::process::id()));
    std::fs::create_dir(&base).unwrap();
    let source = base.join("import.json");
    let imported_home = format!("{}/two", site.base);
    std::fs::write(&source, serde_json::to_vec(&serde_json::json!({"version":1,"settings":{"homepage":imported_home,"show_memory":false,"blocking_exceptions":[site.base]}})).unwrap()).unwrap();
    editor.import_file(&source).unwrap(); // Same file/draft path used by native chooser callback.
    let home = library_widgets(window.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Entry>().ok())
        .find(|entry| entry.widget_name() == "settings-homepage")
        .unwrap();
    assert_eq!(home.text(), imported_home);
    assert!(saved.borrow().is_empty(), "import must not apply or save");
    wait_until("settings window allocation", || {
        window.is_mapped() && window.height() > 0
    });
    let save_button = library_widgets(window.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some("Save settings"))
        .unwrap();
    let bounds = save_button.compute_bounds(&window).unwrap();
    assert!(
        bounds.y() >= 0.0 && bounds.y() + bounds.height() <= window.height() as f32,
        "Save settings must stay visible without scrolling the form"
    );
    capture_native_window(&window, "PAISLEY_SETTINGS_SCREENSHOT");
    let export = base.join("export.json");
    editor.export_file(&export).unwrap();
    assert!(
        !std::fs::read_to_string(&export)
            .unwrap()
            .contains("homepage")
    );
    std::fs::write(&source, br#"{"version":999,"settings":{}}"#).unwrap();
    assert!(editor.import_file(&source).is_err());
    assert_eq!(
        home.text(),
        imported_home,
        "invalid import changed the draft"
    );
    for confirm in [false, true] {
        library_widgets(window.upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            .find(|button| button.label().as_deref() == Some("Reset draft…"))
            .unwrap()
            .emit_clicked();
        let mut dialog = None;
        wait_until("settings reset confirmation", || {
            dialog = gtk::Window::list_toplevels()
                .into_iter()
                .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
                .find(|candidate| {
                    candidate.is_visible() && candidate.transient_for().as_ref() == Some(&window)
                });
            dialog.is_some()
        });
        let dialog = dialog.unwrap();
        let choice = if confirm { "Reset draft" } else { "Cancel" };
        library_widgets(dialog.upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            .find(|button| button.label().as_deref() == Some(choice))
            .unwrap()
            .emit_clicked();
        wait_until("settings reset decision", || !dialog.is_visible());
        assert_eq!(
            home.text().as_str(),
            if confirm {
                "https://duckduckgo.com/"
            } else {
                &imported_home
            }
        );
        assert!(saved.borrow().is_empty(), "reset draft must not save");
    }
    let search = library_widgets(window.upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::SearchEntry>().ok())
        .unwrap();
    search.set_text("memory");
    search.emit_by_name::<()>("search-changed", &[]);
    let matches: Vec<_> = library_widgets(window.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::CheckButton>().ok())
        .filter(|check| check.is_visible())
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "search must retain the matching supported setting"
    );
    assert!(matches[0].label().unwrap().contains("memory"));
    library_widgets(window.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some("Save settings"))
        .unwrap()
        .emit_clicked();
    assert_eq!(
        saved.borrow().as_slice(),
        &[paisley::profile::Settings::default()]
    );
    drop(editor);
    let result = Rc::clone(&saved);
    let editor = crate::settings_ui::show(&parent, current, move |settings| {
        result.borrow_mut().push(settings);
        Ok(())
    });
    let window = gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
        .find(|candidate| {
            candidate.is_visible() && candidate.title().as_deref() == Some("Paisley settings")
        })
        .unwrap();
    window.close();
    drop(editor);
    assert_eq!(
        saved.borrow().len(),
        1,
        "closing a settings draft saved changes"
    );
    let profile = paisley::profile::Profile::open_at(&base, "catalog", false).unwrap();
    drop(profile);
    let catalog = paisley::profile_catalog::ProfileCatalog::at(&base, false);
    let manager =
        crate::settings_ui::profiles_with_catalog(&parent, "fixture", false, Ok(catalog.clone()));
    let name = library_widgets(manager.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Entry>().ok())
        .find(|entry| entry.text() == "catalog")
        .unwrap();
    name.set_text("Owned named profile");
    library_widgets(manager.upcast_ref())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some("Save name"))
        .unwrap()
        .emit_clicked();
    assert_eq!(
        catalog.display_name("catalog").unwrap(),
        "Owned named profile"
    );
    capture_native_window(&manager, "PAISLEY_PROFILES_SCREENSHOT");
    for confirm in [false, true] {
        library_widgets(manager.upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            .find(|button| button.label().as_deref() == Some("Delete profile…"))
            .unwrap()
            .emit_clicked();
        let mut dialog = None;
        wait_until("profile delete confirmation", || {
            dialog = gtk::Window::list_toplevels()
                .into_iter()
                .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
                .find(|candidate| {
                    candidate.is_visible() && candidate.transient_for().as_ref() == Some(&manager)
                });
            dialog.is_some()
        });
        let dialog = dialog.unwrap();
        library_widgets(dialog.upcast_ref())
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
            .find(|button| {
                button.label().as_deref() == Some(if confirm { "Delete profile" } else { "Cancel" })
            })
            .unwrap()
            .emit_clicked();
        wait_until("profile deletion decision", || !dialog.is_visible());
        assert_eq!(base.join("catalog").exists(), !confirm);
    }
    manager.close();
    parent.close();
    std::fs::remove_dir_all(base).unwrap();
}

fn check_download_io_failures(app: &gtk::Application, site: &LocalSite) {
    use paisley::download_history::Outcome;
    use std::os::unix::fs::PermissionsExt;

    let Some(capacity) = std::env::var_os("PAISLEY_DOWNLOAD_CAPACITY_FIXTURE") else {
        eprintln!("SKIP download EACCES/ENOSPC fixtures: use scripts/smoke_isolated.py");
        return;
    };
    let capacity = std::path::PathBuf::from(capacity);
    // Never fill an ordinary filesystem. The isolated runner supplies a new,
    // bounded 1-MiB tmpfs; a mount check precedes the bounded 2-MiB probe.
    assert_eq!(
        capacity,
        std::path::Path::new("/tmp/paisley-download-capacity")
    );
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").unwrap();
    assert!(
        mounts.lines().any(|line| {
            line.split_whitespace().nth(4) == capacity.to_str()
                && line.contains(" - tmpfs ")
                && line.contains("size=1024k")
        }),
        "capacity fixture must be the runner's bounded tmpfs"
    );
    let probe = capacity.join("capacity-probe");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .unwrap();
    let error = file.write_all(&vec![0; 2 * 1024 * 1024]).unwrap_err();
    assert_eq!(
        error.raw_os_error(),
        Some(28),
        "fixture must produce real ENOSPC"
    );
    drop(file);
    std::fs::remove_file(probe).unwrap();

    let base = std::env::temp_dir().join(format!("paisley-download-io-{}", std::process::id()));
    assert!(!base.exists());
    let mut profile = paisley::profile::Profile::open_at(&base, "io", false).unwrap();
    profile.settings.homepage = format!("{}/one", site.base);
    profile.settings.restore_session = false;
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    let view = browser.active_tab().unwrap().view;
    wait_until("download I/O fixture loaded", || {
        !view.is_loading() && view.title().as_deref() == Some("One")
    });

    let denied = base.join("denied");
    std::fs::create_dir(&denied).unwrap();
    std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o500)).unwrap();
    let probe = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(denied.join("probe"));
    assert_eq!(
        probe.unwrap_err().raw_os_error(),
        Some(13),
        "fixture must produce real EACCES, without elevated DAC privileges"
    );

    for (directory, endpoint) in [(&denied, "/download"), (&capacity, "/large-download")] {
        browser.downloads.borrow().shutdown();
        *browser.downloads.borrow_mut() =
            crate::downloads::Downloads::with_directory(&browser.window, directory.clone());
        browser.track_download_changes();
        let downloads = browser.downloads.borrow().clone();
        downloads.show();
        view.download_uri(&format!("{}{endpoint}", site.base))
            .unwrap();
        wait_until("filesystem failure becomes terminal", || {
            !downloads.snapshot().records.is_empty()
        });
        assert_eq!(downloads.active_count(), 0);
        assert!(
            downloads.completed_paths().is_empty(),
            "I/O failure must never be reported as completed"
        );
        assert_eq!(
            downloads.snapshot().records.len(),
            1,
            "failed then finished/cancelled must not duplicate records"
        );
        let record = downloads.snapshot().records[0].clone();
        assert_eq!(record.outcome, Outcome::Failed);
        assert_eq!(
            record.source.as_deref(),
            Some(format!("{}{endpoint}", site.base).as_str())
        );
        assert!(
            !downloads.errors().is_empty(),
            "I/O failure needs a visible error"
        );
        if endpoint == "/large-download" {
            assert!(
                record.destination.is_some(),
                "ENOSPC must happen in native WebKit output, after destination selection"
            );
            assert!(
                downloads
                    .errors()
                    .iter()
                    .any(|error| error.contains("Download failed:")
                        && error.contains("No space left on device")),
                "native write failure must reach the application: {:?}",
                downloads.errors()
            );
        }
        let manager: gtk::Window = browser.window.clone().upcast();
        let widgets = library_widgets(manager.upcast_ref());
        assert!(
            widgets.iter().any(|w| w
                .downcast_ref::<gtk::Label>()
                .is_some_and(|label| label.text().contains(&downloads.errors()[0]))),
            "native download row must explain the filesystem failure"
        );
        assert!(widgets.iter().any(|w| {
            w.downcast_ref::<gtk::ProgressBar>()
                .is_some_and(|p| p.text().as_deref() == Some("Failed"))
        }));
        assert!(!widgets.iter().any(|w| {
            w.downcast_ref::<gtk::Button>()
                .is_some_and(|b| b.is_visible() && b.label().as_deref() == Some("Show in folder"))
        }));
        wait_until("failed download metadata persisted", || {
            browser
                .profile
                .load_download_history()
                .is_ok_and(|history| history.records == downloads.snapshot().records)
        });
        if endpoint == "/download" {
            std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o700)).unwrap();
            widgets
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .find(|b| b.is_visible() && b.label().as_deref() == Some("Retry"))
                .unwrap()
                .emit_clicked();
            wait_until("retry after restoring permissions completes", || {
                !downloads.completed_paths().is_empty()
            });
            assert_eq!(
                std::fs::read(&downloads.completed_paths()[0]).unwrap(),
                b"test"
            );
            assert_eq!(downloads.snapshot().records.len(), 2);
            assert_eq!(downloads.snapshot().records[0].outcome, Outcome::Completed);
            assert_eq!(downloads.snapshot().records[1].outcome, Outcome::Failed);
        }
        eprintln!(
            "PASS download I/O fixture {endpoint}: {:?}",
            downloads.errors()
        );
    }
    browser.window.close();
    drop(view);
    drop(browser);
    let profile = paisley::profile::Profile::open_at(&base, "io", false).unwrap();
    assert_eq!(
        profile.load_download_history().unwrap().records[0].outcome,
        Outcome::Failed
    );
    drop(profile);
    defer_engine_fixture_cleanup_until_process_exit(&base);
}

fn check_download_records(app: &gtk::Application, site: &LocalSite) {
    check_download_io_failures(app, site);
    use paisley::download_history::Outcome;
    let base = std::env::temp_dir().join(format!(
        "paisley-record-integration-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for (id, private) in [("work", false), ("private", true)] {
        let mut profile = paisley::profile::Profile::open_at(&base, id, private).unwrap();
        profile.settings.homepage = format!("{}/one", site.base);
        profile.settings.restore_session = false;
        let directory = base.join(format!("{id}-files"));
        std::fs::create_dir(&directory).unwrap();
        let browser = Browser::with_profile(app, profile).unwrap();
        browser.present();
        let view = browser.active_tab().unwrap().view;
        wait_until("download record fixture", || {
            !view.is_loading() && view.title().as_deref() == Some("One")
        });
        *browser.downloads.borrow_mut() =
            crate::downloads::Downloads::with_directory(&browser.window, directory.clone());
        browser.track_download_changes();
        view.download_uri(&format!("{}/download", site.base))
            .unwrap();
        wait_until("recorded attachment complete", || {
            !browser.downloads.borrow().completed_paths().is_empty()
        });
        let history = browser.downloads.borrow().snapshot();
        assert_eq!(
            history.records.len(),
            1,
            "completed attachments need metadata"
        );
        let record = &history.records[0];
        assert_eq!(record.outcome, Outcome::Completed);
        assert_eq!(record.received, 4);
        assert_eq!(
            record.source.as_deref(),
            Some(format!("{}/download", site.base).as_str())
        );
        let path = std::path::PathBuf::from(record.destination.as_ref().unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"test");
        if !private {
            wait_until("download metadata autosaved", || {
                browser
                    .profile
                    .load_download_history()
                    .is_ok_and(|h| h.records.len() == 1)
            });
        }
        view.download_uri(&format!("{}/abort", site.base)).unwrap();
        wait_until("pre-response download failure recorded", || {
            browser.downloads.borrow().snapshot().records.len() == 2
        });
        let failed = browser.downloads.borrow().snapshot().records[0].clone();
        assert_eq!(failed.outcome, Outcome::Failed);
        assert!(failed.destination.is_none());
        let truncated = view
            .download_uri(&format!("{}/broken-download", site.base))
            .unwrap();
        wait_until("truncated transfer terminated", || {
            browser.downloads.borrow().snapshot().records.len() >= 3
        });
        assert_eq!(truncated.response().unwrap().content_length(), 16);
        assert_eq!(truncated.received_data_length(), 4);
        assert!(
            truncated
                .response()
                .unwrap()
                .http_headers()
                .unwrap()
                .one("Content-Encoding")
                .is_none()
        );
        assert_eq!(
            browser.downloads.borrow().snapshot().records[0].outcome,
            Outcome::Failed,
            "truncated fixture outcome: {:?}; errors: {:?}",
            browser.downloads.borrow().snapshot(),
            browser.downloads.borrow().errors()
        );
        view.download_uri(&format!("{}/pending-download", site.base))
            .unwrap();
        wait_until("pending transfer started", || {
            browser.downloads.borrow().active_count() == 1
        });
        let downloads = browser.downloads.borrow().clone();
        browser.window.close();
        let history = downloads.snapshot();
        assert_eq!(history.records.len(), 4);
        assert_eq!(
            history.records[0].outcome,
            Outcome::Cancelled,
            "close cancels active downloads before final snapshot"
        );
        drop(downloads);
        drop(browser);
        drop(view);
        if private {
            assert!(
                !base.join(id).exists(),
                "private download metadata must not be persisted"
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                b"test",
                "explicitly saved private files survive"
            );
        } else {
            let mut profile = paisley::profile::Profile::open_at(&base, id, false).unwrap();
            assert_eq!(profile.load_download_history().unwrap(), history);
            profile.settings.homepage = format!("{}/one", site.base);
            profile.settings.restore_session = false;
            let browser = Browser::with_profile(app, profile).unwrap();
            browser.present();
            gtk::prelude::WidgetExt::activate_action(&browser.window, "win.downloads", None)
                .unwrap();
            let manager: gtk::Window = browser.window.clone().upcast();
            assert!(
                library_widgets(manager.upcast_ref()).iter().any(|w| w
                    .downcast_ref::<gtk::Label>()
                    .is_some_and(|l| l
                        .tooltip_text()
                        .is_some_and(|text| text.contains(path.to_str().unwrap())))),
                "restored metadata should be visible without redownloading"
            );
            capture_native_window(&manager, "PAISLEY_DOWNLOADS_SCREENSHOT");
            std::fs::remove_file(&path).unwrap();
            let reveal = library_widgets(manager.upcast_ref())
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .find(|b| b.label().as_deref() == Some("Show in folder"))
                .unwrap();
            reveal.emit_clicked(); // Missing-file validation must stop before any external launcher.
            wait_until("missing saved download explained", || {
                library_widgets(manager.upcast_ref()).iter().any(|w| {
                    w.downcast_ref::<gtk::Label>()
                        .is_some_and(|l| l.text().contains("Cannot reveal"))
                })
            });
            std::fs::write(&path, b"test").unwrap(); // Test-owned file: clearing metadata must leave it alone.
            for confirm in [false, true] {
                let clear = library_widgets(manager.upcast_ref())
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Button>().ok())
                    .find(|b| b.label().as_deref() == Some("Clear history…"))
                    .unwrap();
                clear.emit_clicked();
                let mut dialog = None;
                wait_until("download-record clear confirmation", || {
                    dialog = gtk::Window::list_toplevels()
                        .into_iter()
                        .filter_map(|w| w.downcast::<gtk::Window>().ok())
                        .find(|w| w.is_visible() && w.transient_for().as_ref() == Some(&manager));
                    dialog.is_some()
                });
                let dialog = dialog.unwrap();
                let choice = if confirm { "Clear records" } else { "Cancel" };
                library_widgets(dialog.upcast_ref())
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Button>().ok())
                    .find(|b| b.label().as_deref() == Some(choice))
                    .unwrap()
                    .emit_clicked();
                wait_until("download clear dialog dismissed", || !dialog.is_visible());
                if confirm {
                    wait_until("download records cleared", || {
                        browser.downloads.borrow().snapshot().records.is_empty()
                    });
                } else {
                    assert_eq!(browser.downloads.borrow().snapshot(), history);
                }
                assert_eq!(std::fs::read(&path).unwrap(), b"test");
            }
            browser.window.close();
            drop(browser);
            let profile = paisley::profile::Profile::open_at(&base, id, false).unwrap();
            assert!(profile.load_download_history().unwrap().records.is_empty());
        }
    }
    let mut profile = paisley::profile::Profile::open_at(&base, "eviction", false).unwrap();
    profile.settings.homepage = format!("{}/one", site.base);
    profile.settings.restore_session = false;
    let mut history = paisley::download_history::DownloadHistory::default();
    for i in 0..200 {
        history.record(
            Some(&format!("{}/saved/{i}", site.base)),
            None,
            Outcome::Failed,
            0,
            i,
        );
    }
    profile.save_download_history(&history).unwrap();
    let browser = Browser::with_profile(app, profile).unwrap();
    let directory = base.join("eviction-files");
    std::fs::create_dir(&directory).unwrap();
    browser.downloads.borrow().shutdown();
    *browser.downloads.borrow_mut() =
        crate::downloads::Downloads::with_directory_history(&browser.window, directory, history);
    browser.track_download_changes();
    browser.present();
    browser.downloads.borrow().show();
    browser
        .active_tab()
        .unwrap()
        .view
        .download_uri(&format!("{}/download", site.base))
        .unwrap();
    wait_until("download causing metadata eviction", || {
        !browser.downloads.borrow().completed_paths().is_empty()
    });
    let snapshot = browser.downloads.borrow().snapshot();
    assert_eq!(snapshot.records.len(), 200);
    let old = format!("{}/saved/0", site.base);
    assert!(
        !snapshot
            .records
            .iter()
            .any(|r| r.source.as_deref() == Some(old.as_str()))
    );
    let manager: gtk::Window = browser.window.clone().upcast();
    assert!(
        !library_widgets(manager.upcast_ref()).iter().any(|w| w
            .downcast_ref::<gtk::Label>()
            .is_some_and(|l| l.text().ends_with(&old))),
        "evicted records must disappear from the saved-record UI, not just disk snapshots"
    );
    browser.window.close();
    drop(browser);
    for (id, bad) in [
        ("corrupt-downloads", "{"),
        ("future-downloads", "{\"version\":999,\"records\":[]}"),
    ] {
        let mut profile = paisley::profile::Profile::open_at(&base, id, false).unwrap();
        profile.settings.homepage = format!("{}/one", site.base);
        profile.settings.restore_session = false;
        let path = profile.data_dir.join("downloads.json");
        std::fs::write(&path, bad).unwrap();
        let opened = Browser::with_profile(app, profile);
        assert!(
            opened.is_ok(),
            "damaged nonessential download metadata must not block browsing"
        );
        let browser = opened.unwrap();
        browser.present();
        wait_until("degraded profile opens fixture", || {
            browser.active_tab().unwrap().view.title().as_deref() == Some("One")
        });
        gtk::prelude::WidgetExt::activate_action(&browser.window, "win.downloads", None).unwrap();
        let manager: gtk::Window = browser.window.clone().upcast();
        assert!(
            library_widgets(manager.upcast_ref()).iter().any(|w| w
                .downcast_ref::<gtk::Label>()
                .is_some_and(
                    |l| l.text().contains("memory-only") && l.text().contains("preserved")
                ))
        );
        browser
            .active_tab()
            .unwrap()
            .view
            .download_uri(&format!("{}/abort", site.base))
            .unwrap();
        wait_until("memory-only download failure", || {
            browser.downloads.borrow().snapshot().records.len() == 1
        });
        browser.window.close();
        drop(browser);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            bad,
            "neither automatic writes nor close may replace the bad record"
        );
        let profile = paisley::profile::Profile::open_at(&base, id, false).unwrap();
        assert!(profile.load_session().is_ok());
        assert!(profile.load_library().is_ok());
    }
    defer_engine_fixture_cleanup_until_process_exit(&base);
}

fn library_widgets(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut result = vec![root.clone()];
    let mut child = root.first_child();
    while let Some(widget) = child {
        result.extend(library_widgets(&widget));
        child = widget.next_sibling();
    }
    result
}

fn check_browser_library(app: &gtk::Application, site: &LocalSite) {
    let base = std::env::temp_dir().join(format!(
        "paisley-browser-library-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut profile = paisley::profile::Profile::open_at(&base, "work", false).unwrap();
    profile.settings.homepage = format!("{}/one", site.base);
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    let view = browser.active_tab().unwrap().view;
    wait_until("library first page", || {
        view.title().as_deref() == Some("One") && !view.is_loading()
    });
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.bookmark-current", None)
        .expect("bookmark action");
    save_bookmark_draft(&browser);
    browser.navigate(&format!("{}/two", site.base));
    wait_until("library second page", || {
        view.title().as_deref() == Some("Two") && !view.is_loading()
    });
    for (action, title, page_title) in [
        ("win.bookmarks", "Bookmarks", "One"),
        ("win.history", "History", "Two"),
    ] {
        gtk::prelude::WidgetExt::activate_action(&browser.window, action, None).unwrap();
        let manager: gtk::Window = browser.window.clone().upcast();
        assert!(library_widgets(manager.upcast_ref()).iter().any(|w| {
            w.downcast_ref::<gtk::Label>()
                .is_some_and(|l| l.text() == title)
        }));
        wait_until("library sheet mapped", || {
            library_widgets(&library_sheet_panel(&browser))
                .iter()
                .any(|w| {
                    w.downcast_ref::<gtk::SearchEntry>()
                        .is_some_and(|w| w.is_mapped())
                })
        });
        wait_until("library manager visible", || manager.is_visible());
        let widgets = library_widgets(&library_sheet_panel(&browser));
        let search = widgets
            .into_iter()
            .find_map(|w| w.downcast::<gtk::SearchEntry>().ok())
            .unwrap();
        search.set_text("no-such-title-in-this-fixture");
        wait_until("library search hides nonmatches", || {
            library_widgets(&library_sheet_panel(&browser))
                .iter()
                .any(|w| {
                    w.downcast_ref::<gtk::Label>()
                        .is_some_and(|l| l.text().starts_with("0 results"))
                })
        });
        assert!(
            !library_widgets(&library_sheet_panel(&browser))
                .iter()
                .any(|w| {
                    w.downcast_ref::<gtk::Label>()
                        .is_some_and(|l| l.text() == page_title)
                })
        );
        search.set_text(page_title);
        wait_until("library search shows matches", || {
            library_widgets(&library_sheet_panel(&browser))
                .iter()
                .any(|w| {
                    w.downcast_ref::<gtk::Label>()
                        .is_some_and(|l| l.text() == page_title)
                })
        });
        wait_until("library filtered count", || {
            library_widgets(&library_sheet_panel(&browser))
                .iter()
                .any(|w| {
                    w.downcast_ref::<gtk::Label>()
                        .is_some_and(|l| l.text().starts_with("1 result"))
                })
        });
        if action == "win.history" {
            assert!(
                !library_widgets(&library_sheet_panel(&browser))
                    .iter()
                    .any(|w| {
                        w.downcast_ref::<gtk::Label>()
                            .is_some_and(|l| l.text() == "One")
                    })
            );
            capture_native_window(&manager, "PAISLEY_LIBRARY_SCREENSHOT");
        }
        browser.menu.dismiss();
    }
    for relative in ["/two?route=spa", "/two?route=spa#section", "/two"] {
        javascript(
            &view,
            &format!("history.pushState(null, '', '{relative}'); 'navigated'"),
        );
        wait_until("same-document visit recorded", || {
            browser
                .library
                .borrow()
                .history
                .first()
                .is_some_and(|p| p.uri == format!("{}{relative}", site.base))
        });
    }
    let before_failure = browser.library.borrow().history.clone();
    browser.navigate(&format!("{}/abort", site.base));
    wait_until("uncommitted navigation failed", || {
        browser.active_tab().unwrap().error.is_visible() && !view.is_loading()
    });
    assert_eq!(
        browser.library.borrow().history,
        before_failure,
        "failed provisional destinations must not create history entries"
    );
    browser.navigate(&format!("{}/unfinished", site.base));
    wait_until("committed page with pending subresource", || {
        view.title().as_deref() == Some("Unfinished")
            && view.is_loading()
            && browser.active_tab().unwrap().committed.get()
    });
    assert!(
        browser
            .library
            .borrow()
            .history
            .iter()
            .any(|p| p.uri == format!("{}/unfinished", site.base)),
        "visible committed pages must be recorded before Finished"
    );
    browser.window.close();
    drop(browser);
    drop(view);
    let profile = paisley::profile::Profile::open_at(&base, "work", false).unwrap();
    let library = profile.load_library().unwrap();
    assert_eq!(library.bookmarks.len(), 1);
    assert_eq!(library.bookmarks[0].uri, format!("{}/one", site.base));
    assert_eq!(library.bookmarks[0].title, "One");
    assert_eq!(library.history[0].uri, format!("{}/unfinished", site.base));
    assert!(
        library
            .history
            .iter()
            .any(|p| p.uri == format!("{}/two", site.base))
    );
    assert!(
        library
            .history
            .iter()
            .any(|p| p.uri == format!("{}/one", site.base))
    );
    drop(profile);
    for (id, private) in [("no-history", false), ("private", true)] {
        let mut profile = paisley::profile::Profile::open_at(&base, id, private).unwrap();
        profile.settings.homepage = format!("{}/one", site.base);
        profile.settings.remember_history = private; // Test both opt-out and private overriding opt-in.
        profile.save_settings(&profile.settings).unwrap();
        let browser = Browser::with_profile(app, profile).unwrap();
        browser.present();
        let view = browser.active_tab().unwrap().view;
        wait_until("nonrecording page", || {
            view.title().as_deref() == Some("One") && !view.is_loading()
        });
        assert!(browser.library.borrow().history.is_empty());
        gtk::prelude::WidgetExt::activate_action(&browser.window, "win.bookmark-current", None)
            .unwrap();
        save_bookmark_draft(&browser);
        assert_eq!(browser.library.borrow().bookmarks.len(), 1);
        browser.window.close();
        drop(browser);
        drop(view);
        if private {
            assert!(!base.join(id).exists());
        } else {
            let profile = paisley::profile::Profile::open_at(&base, id, false).unwrap();
            assert!(!profile.settings.remember_history);
            let saved = profile.load_library().unwrap();
            assert!(saved.history.is_empty());
            assert_eq!(saved.bookmarks.len(), 1);
        }
    }
    // A failed background write stays pending and retries without another user edit.
    let mut profile = paisley::profile::Profile::open_at(&base, "retry", false).unwrap();
    profile.settings.homepage = format!("{}/one", site.base);
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    let tab = browser.active_tab().unwrap();
    wait_until("retry fixture loaded and saved", || {
        !tab.view.is_loading()
            && tab.view.title().as_deref() == Some("One")
            && browser.profile.data_dir.join("session.json").exists()
            && !browser.dirty.get()
    });
    let path = browser.profile.data_dir.join("session.json");
    let original = std::fs::read(&path).unwrap();
    std::fs::write(&path, "{").unwrap();
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.bookmark-current", None)
        .unwrap();
    save_bookmark_draft(&browser);
    wait_until("background save error reported", || {
        tab.error.text().contains("Could not save")
    });
    let retry_pending = browser.dirty.get();
    // Restore only this test-owned deliberately corrupt record, even if the assertion fails.
    std::fs::write(&path, original).unwrap();
    assert!(
        retry_pending,
        "a failed snapshot must be retried without another edit"
    );
    wait_until("repaired snapshot saved automatically", || {
        browser
            .profile
            .load_library()
            .is_ok_and(|l| l.bookmarks.len() == 1)
            && !browser.dirty.get()
    });
    for (action, title, button) in [
        ("win.bookmarks", "Bookmarks", "Remove"),
        ("win.history", "History", "Clear history…"),
    ] {
        gtk::prelude::WidgetExt::activate_action(&browser.window, action, None).unwrap();
        let manager: gtk::Window = browser.window.clone().upcast();
        assert!(library_widgets(manager.upcast_ref()).iter().any(|w| {
            w.downcast_ref::<gtk::Label>()
                .is_some_and(|l| l.text() == title)
        }));
        wait_until("library sheet mapped", || {
            library_widgets(&library_sheet_panel(&browser))
                .iter()
                .any(|w| {
                    w.downcast_ref::<gtk::SearchEntry>()
                        .is_some_and(|w| w.is_mapped())
                })
        });
        if action == "win.bookmarks" {
            library_widgets(&library_sheet_panel(&browser))
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::ToggleButton>().ok())
                .find(|b| b.tooltip_text().as_deref() == Some("Edit bookmark"))
                .unwrap()
                .set_active(true);
            library_widgets(&library_sheet_panel(&browser))
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .find(|b| b.label().as_deref() == Some("Remove"))
                .unwrap()
                .emit_clicked();
            assert!(browser.library.borrow().bookmarks.is_empty());
            browser.menu.dismiss();
            continue;
        }
        for confirm in [false, true] {
            let trigger = library_widgets(&library_sheet_panel(&browser))
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .find(|b| b.label().as_deref() == Some(button))
                .unwrap();
            trigger.emit_clicked();
            let mut dialog = None;
            wait_until("library destructive-action confirmation", || {
                dialog = gtk::Window::list_toplevels()
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Window>().ok())
                    .find(|w| w.is_visible() && w.transient_for().as_ref() == Some(&manager));
                dialog.is_some()
            });
            let dialog = dialog.unwrap();
            let choice = if confirm { "Remove" } else { "Cancel" };
            let choice = library_widgets(dialog.upcast_ref())
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::Button>().ok())
                .find(|b| b.label().as_deref() == Some(choice))
                .unwrap();
            choice.emit_clicked();
            wait_until("library confirmation dismissed", || !dialog.is_visible());
            let empty = || {
                let library = browser.library.borrow();
                if action == "win.bookmarks" {
                    library.bookmarks.is_empty()
                } else {
                    library.history.is_empty()
                }
            };
            if confirm {
                wait_until("confirmed library change", empty);
            } else {
                assert!(!empty(), "Cancel must preserve library data");
            }
        }
        browser.menu.dismiss();
    }
    browser.window.close();
    drop(browser);
    drop(tab);
    let profile = paisley::profile::Profile::open_at(&base, "retry", false).unwrap();
    let saved = profile.load_library().unwrap();
    assert!(saved.history.is_empty());
    assert!(saved.bookmarks.is_empty());
    drop(profile);
    defer_engine_fixture_cleanup_until_process_exit(&base);
}

fn check_sensitive_permission_denial(app: &gtk::Application, site: &LocalSite) {
    let browser = Browser::new(app);
    browser.present();
    wait_until("permission fixture protection", || {
        browser.blocker.ready.get()
    });
    browser.navigate(&format!("{}/one", site.base));
    let tab = browser.active_tab().unwrap();
    wait_until("permission fixture loaded", || {
        tab.view.title().as_deref() == Some("One") && !tab.view.is_loading()
    });
    javascript(
        &tab.view,
        "window.permissionResult='pending'; navigator.geolocation.getCurrentPosition(()=>window.permissionResult='granted',e=>window.permissionResult=String(e.code)); 'requested'",
    );
    wait_until("explicit permission-denial explanation", || {
        tab.error.is_visible() && tab.error.text().contains("requesting origin")
    });
    wait_until("native permission denied", || {
        javascript(&tab.view, "window.permissionResult") == "1"
    });
    assert!(tab.error.text().contains("blocked"));
    assert!(!tab.error.text().contains("granted"));
    browser.window.close();
}

fn check_native_request_blocking(app: &gtk::Application, site: &LocalSite) {
    let profile = paisley::profile::Profile::open("filter-test", true).unwrap();
    let blocker = crate::blocking::Blocker::new(&profile).unwrap();
    blocker.compile(
        r#"[{"trigger":{"url-filter":".*/tracker.js"},"action":{"type":"block"}}]"#,
        || {},
    );
    wait_until("filter compilation", || blocker.ready.get());
    assert!(
        blocker.filter().is_some(),
        "native content filter must compile: {:?}",
        blocker.error.borrow()
    );
    let browser = Browser::new(app);
    browser.present();
    let view = browser.active_tab().unwrap().view;
    view.user_content_manager()
        .unwrap()
        .add_filter(&blocker.filter().unwrap());
    let before = site.trackers.load(Ordering::Relaxed);
    browser.navigate(&format!("{}/blocking", site.base));
    wait_until("blocking fixture", || {
        javascript(&view, "window.allowedScript === true") == "true"
    });
    assert_eq!(
        site.trackers.load(Ordering::Relaxed),
        before,
        "blocked script must not reach the fixture server"
    );
    view.user_content_manager()
        .unwrap()
        .remove_filter(&blocker.filter().unwrap());
    view.reload();
    wait_until("allowed tracker after exception", || {
        site.trackers.load(Ordering::Relaxed) > before
    });
    browser.window.close();
    check_blocking_navigation_boundaries(app, site);
}

fn check_blocking_navigation_boundaries(app: &gtk::Application, site: &LocalSite) {
    let browser = Browser::new(app);
    browser.present();
    wait_until("starter protection ready", || browser.blocker.ready.get());
    browser.blocker.compile(
        r#"[{"trigger":{"url-filter":".*/tracker.js"},"action":{"type":"block"}}]"#,
        || {},
    );
    wait_until("boundary fixture filter", || browser.blocker.ready.get());
    browser.apply_settings();
    let view = browser.active_tab().unwrap().view;
    let before = site.trackers.load(Ordering::Relaxed);
    let related = browser.add_tab(Some(&view), None);
    related.load_uri(&format!("{}/blocking", site.base));
    wait_until("page-opened related tab", || {
        related.title().as_deref() == Some("Blocking fixture") && !related.is_loading()
    });
    assert_eq!(
        site.trackers.load(Ordering::Relaxed),
        before,
        "page-opened tabs must inherit protection before navigation"
    );
    let a = site.base.clone();
    let b = site.base.replace("127.0.0.1", "localhost");
    browser
        .settings
        .borrow_mut()
        .blocking_exceptions
        .push(a.clone());
    browser.apply_settings();
    let exception = format!("{a}/blocking?from=exception");
    browser.navigate(&exception);
    wait_until("exception allows tracker", || {
        site.trackers.load(Ordering::Relaxed) > before
            && !related.is_loading()
            && related
                .back_forward_list()
                .and_then(|list| list.current_item())
                .is_some_and(|item| item.uri().as_deref() == Some(exception.as_str()))
    });
    let allowed = site.trackers.load(Ordering::Relaxed);
    let target = format!("{b}/blocking?from=link");
    related.load_uri(&target); // Engine-driven navigation, bypassing address-bar helper.
    let navigation_started = Instant::now();
    let dumped = Cell::new(false);
    wait_until("cross-origin destination loaded", || {
        if navigation_started.elapsed() > Duration::from_secs(19) && !dumped.replace(true) {
            let list = related.back_forward_list();
            eprintln!(
                "boundary diagnostic uri={:?} loading={} current={:?} back={:?} error={:?}",
                related.uri(),
                related.is_loading(),
                list.as_ref()
                    .and_then(|l| l.current_item())
                    .and_then(|i| i.uri()),
                list.as_ref()
                    .and_then(|l| l.back_item())
                    .and_then(|i| i.uri()),
                browser.active_tab().map(|t| t.error.text())
            );
        }
        related.uri().as_deref() == Some(target.as_str())
            && !related.is_loading()
            && related
                .back_forward_list()
                .and_then(|list| list.current_item())
                .is_some_and(|item| item.uri().as_deref() == Some(target.as_str()))
    });
    assert_eq!(
        site.trackers.load(Ordering::Relaxed),
        allowed,
        "site A exception must not leak to site B"
    );
    related.go_back();
    wait_until("history returns to exception origin", || {
        related.uri().is_some_and(|uri| uri.starts_with(a.as_str())) && !related.is_loading()
    });
    javascript(
        &related,
        "window.probe=null; fetch('/tracker.js?back', {cache:'no-store'}).then(()=>window.probe='allowed',()=>window.probe='blocked'); 'queued'",
    );
    wait_until("exception applies after history back", || {
        javascript(&related, "window.probe") == "allowed"
    });
    let after_back = site.trackers.load(Ordering::Relaxed);
    related.go_forward();
    wait_until("history returns to protected origin", || {
        related.uri().as_deref() == Some(target.as_str()) && !related.is_loading()
    });
    javascript(
        &related,
        "window.probe=null; fetch('/tracker.js?forward', {cache:'no-store'}).then(()=>window.probe='allowed',()=>window.probe='blocked'); 'queued'",
    );
    wait_until("protection applies after history forward", || {
        javascript(&related, "window.probe") == "blocked"
    });
    assert_eq!(site.trackers.load(Ordering::Relaxed), after_back);
    browser.window.close();
}

fn check_non_restorable_sessions(app: &gtk::Application, site: &LocalSite) {
    use paisley::profile::{NON_RESTORABLE_TAB, Profile, Session};
    let root = std::env::temp_dir().join(format!("paisley-placeholder-{}", std::process::id()));
    assert!(!root.exists());
    let first_uri = format!("{}/one", site.base);
    let last_uri = format!("{}/two", site.base);
    let transient =
        "data:text/html,<title>Temporary fixture</title><p>SECRET_TRANSIENT_PAYLOAD</p>";
    let expected = Session {
        tabs: vec![
            first_uri.clone(),
            NON_RESTORABLE_TAB.into(),
            last_uri.clone(),
        ],
        active: 1,
    };
    let mut profile = Profile::open_at(&root, "owned", false).unwrap();
    profile.settings.homepage = first_uri.clone();
    profile
        .save_session(&Session {
            tabs: vec![first_uri.clone(), transient.into(), last_uri.clone()],
            active: 1,
        })
        .unwrap();
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    wait_until("placeholder blocker initialized", || {
        browser.blocker.ready.get()
    });
    assert_eq!(browser.notebook.n_pages(), 3);
    assert_eq!(browser.notebook.current_page(), Some(1));
    let tab = browser.active_tab().unwrap();
    assert_eq!(
        tab.title.text().as_str(),
        "Not restored",
        "transient tab needs a native placeholder, not a failed WebKit navigation"
    );
    assert!(!tab.view.is_visible());
    assert!(browser.address.text().is_empty());
    assert!(!browser.reload.is_sensitive());
    assert!(library_widgets(tab.page.upcast_ref()).iter().any(|widget| {
        widget.downcast_ref::<gtk::Label>().is_some_and(|label| {
            label.is_visible()
                && !label.uses_markup()
                && label.text().contains("original address were not saved")
        })
    }));
    for tab in browser.tabs.borrow().iter() {
        assert!(tab.view.uri().is_none_or(|uri| uri == "about:blank"));
        assert!(
            !tab.view.is_loading(),
            "restored background tabs must stay lazy"
        );
    }
    assert!(browser.library.borrow().history.is_empty());
    assert_eq!(browser.snapshot_session(), expected);
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.reload", None).unwrap();
    assert_eq!(browser.snapshot_session(), expected);
    assert!(!tab.view.is_loading());
    capture_native_window(
        browser.window.upcast_ref(),
        "PAISLEY_PLACEHOLDER_SCREENSHOT",
    );
    browser.window.close();
    drop(tab);
    drop(browser);
    let profile = Profile::open_at(&root, "owned", false).unwrap();
    assert_eq!(profile.load_session().unwrap(), expected);
    let record = std::fs::read_to_string(profile.data_dir.join("session.json")).unwrap();
    assert!(!record.contains("SECRET_TRANSIENT_PAYLOAD") && !record.contains("data:text/html"));
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    let tab = browser.active_tab().unwrap();
    assert_eq!(tab.title.text().as_str(), "Not restored");
    browser.notebook.set_current_page(Some(2));
    wait_until("safe background tab activates normally", || {
        browser.active_tab().unwrap().view.title().as_deref() == Some("Two")
    });
    browser.notebook.set_current_page(Some(1));
    assert_eq!(browser.active_tab().unwrap().id, tab.id);
    assert_eq!(tab.title.text().as_str(), "Not restored");
    browser.navigate(NON_RESTORABLE_TAB);
    assert!(
        tab.error.is_visible(),
        "metadata marker must not become address-bar navigation authority"
    );
    assert_eq!(browser.snapshot_session(), expected);
    browser.navigate(&first_uri);
    wait_until("explicit navigation replaces placeholder", || {
        tab.view.title().as_deref() == Some("One") && !tab.view.is_loading()
    });
    assert!(tab.view.is_visible() && browser.reload.is_sensitive());
    assert_eq!(browser.snapshot_session().tabs[1], first_uri);
    // Exercise an actual transient engine document, not only seeded metadata.
    tab.view.load_uri(transient);
    wait_until("actual transient document", || {
        tab.view.title().as_deref() == Some("Temporary fixture") && !tab.view.is_loading()
    });
    assert_eq!(
        browser.snapshot_session(),
        expected,
        "queued session metadata must already be payload-free"
    );
    browser.window.close();
    drop(tab);
    drop(browser);
    let profile = Profile::open_at(&root, "owned", false).unwrap();
    assert_eq!(profile.load_session().unwrap(), expected);
    let record = std::fs::read_to_string(profile.data_dir.join("session.json")).unwrap();
    assert!(!record.contains("SECRET_TRANSIENT_PAYLOAD"));
    drop(profile);
    defer_engine_fixture_cleanup_until_process_exit(&root);
}

fn log_inspector_ancestors(view: &webkit::WebViewBase) {
    let mut widget = Some(view.clone().upcast::<gtk::Widget>());
    let mut depth = 0;
    while let Some(current) = widget {
        let allocation = current.allocation();
        eprintln!(
            "Inspector ancestor {depth}: type={}, visible={}, mapped={}, allocation={}x{}",
            current.type_().name(),
            current.is_visible(),
            current.is_mapped(),
            allocation.width(),
            allocation.height()
        );
        widget = current.parent();
        depth += 1;
        if depth == 32 {
            eprintln!("Inspector ancestor chain truncated");
            break;
        }
    }
}

fn mapped_inspector(inspector: &webkit::WebInspector) -> webkit::WebViewBase {
    // A just-created frontend can still have a floating native reference.
    // glib's from_glib_none sinks it: retain that first Rust wrapper until the
    // engine has parented/mapped the widget instead of dropping it while polling.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut frontend = None;
    let mut last_state = None;
    loop {
        while gtk::glib::MainContext::default().pending() {
            gtk::glib::MainContext::default().iteration(false);
        }
        if frontend.is_none() {
            frontend = inspector.web_view();
        }
        if let Some(view) = frontend.as_ref() {
            let root = view.root().map(|root| root.type_().name().to_owned());
            let state = (
                inspector.is_attached(),
                view.is_visible(),
                view.is_mapped(),
                view.width(),
                view.height(),
                root,
            );
            if last_state.as_ref() != Some(&state) {
                eprintln!(
                    "Inspector mapping state: attached={}, visible={}, mapped={}, size={}x{}, root={:?}",
                    state.0, state.1, state.2, state.3, state.4, state.5
                );
                if view.is_mapped() && (view.width() == 0 || view.height() == 0) {
                    log_inspector_ancestors(view);
                }
                last_state = Some(state);
            }
            if view.is_mapped() && view.width() > 0 && view.height() > 0 {
                return frontend.unwrap();
            }
        } else if last_state.is_none() {
            eprintln!(
                "Inspector mapping state: frontend unavailable, attached={}",
                inspector.is_attached()
            );
            last_state = Some((false, false, false, 0, 0, None));
        }
        if Instant::now() >= deadline {
            if let Some(view) = frontend.as_ref() {
                eprintln!(
                    "Inspector mapping timeout: attached={}, visible={}, mapped={}, size={}x{}, rooted={}",
                    inspector.is_attached(),
                    view.is_visible(),
                    view.is_mapped(),
                    view.width(),
                    view.height(),
                    view.root().is_some()
                );
                log_inspector_ancestors(view);
            } else {
                eprintln!(
                    "Inspector mapping timeout: frontend unavailable, attached={}",
                    inspector.is_attached()
                );
            }
            isolated_resource_snapshot();
            panic!("timed out waiting for native Inspector mapped");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn check_inspector_open_close(app: &gtk::Application, site: &LocalSite) {
    let base = std::env::temp_dir().join(format!(
        "paisley-inspector-open-close-{}",
        std::process::id()
    ));
    assert!(!base.exists());
    let mut profile = paisley::profile::Profile::open_at(&base, "owned", true).unwrap();
    profile.settings.homepage = format!("{}/one", site.base);
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    let tab = browser.active_tab().unwrap();
    wait_until("Inspector reopen fixture", || {
        tab.view.title().as_deref() == Some("One") && !tab.view.is_loading()
    });
    let inspector = tab.view.inspector().unwrap();
    let closed = Rc::new(Cell::new(0));
    let count = closed.clone();
    inspector.connect_closed(move |_| count.set(count.get() + 1));
    for cycle in 1..=3 {
        gtk::prelude::WidgetExt::activate_action(&browser.window, "win.inspector", None).unwrap();
        let frontend = mapped_inspector(&inspector);
        let previous_closed = closed.get();
        inspector.close();
        wait_until("Inspector open-close cycle", || {
            closed.get() > previous_closed
                && !frontend.is_mapped()
                && frontend.root().is_none()
                && inspector.web_view().is_none()
        });
        eprintln!("Inspector simple open-close cycle {cycle}: closed");
    }
    browser.window.close();
    assert!(
        !base.exists(),
        "private Inspector fixture must not persist a profile"
    );
}

fn check_inspector_lifecycle(app: &gtk::Application, site: &LocalSite) {
    // These assertions exercise real native Inspector lifecycle, not page-side
    // JavaScript pretending to be console, DOM-editor or debugger coverage.
    let base = std::env::temp_dir().join(format!("paisley-inspector-{}", std::process::id()));
    assert!(!base.exists());
    let mut profile = paisley::profile::Profile::open_at(&base, "owned", true).unwrap();
    profile.settings.homepage = format!("{}/one", site.base);
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    let first = browser.active_tab().unwrap();
    wait_until("Inspector owner fixture", || {
        first.view.title().as_deref() == Some("One") && !first.view.is_loading()
    });
    let inspector = first.view.inspector().unwrap();
    let closed = Rc::new(Cell::new(0));
    let count = closed.clone();
    inspector.connect_closed(move |_| count.set(count.get() + 1));
    let owner_root = browser.window.clone().upcast::<gtk::Root>();
    for cycle in 1..=3 {
        let prior_frontend = inspector.web_view();
        eprintln!(
            "Inspector before open {cycle}: attached={}, frontend={:?}",
            inspector.is_attached(),
            prior_frontend.as_ref().map(|view| (
                view.is_visible(),
                view.is_mapped(),
                view.root().is_some()
            ))
        );
        gtk::prelude::WidgetExt::activate_action(&browser.window, "win.inspector", None).unwrap();
        let frontend = mapped_inspector(&inspector);
        eprintln!(
            "Inspector geometry cycle {cycle}: owner={}x{}, page={}x{}, can_attach={}, attached={}, frontend={}x{}",
            browser.window.width(),
            browser.window.height(),
            first.view.width(),
            first.view.height(),
            inspector.can_attach(),
            inspector.is_attached(),
            frontend.width(),
            frontend.height()
        );
        if inspector.is_attached() {
            inspector.detach();
            wait_until("initially docked Inspector detaches", || {
                !inspector.is_attached()
                    && frontend.is_mapped()
                    && frontend.root().is_some_and(|root| root != owner_root)
            });
        }
        // Exercise the actual attach transition. The installed engine reports
        // can_attach=false even while initially attached; record it above rather
        // than substitute that advisory property for the lifecycle assertion.
        inspector.attach();
        wait_until("native Inspector docked", || {
            inspector.is_attached()
                && frontend.is_mapped()
                && frontend.root().as_ref() == Some(&owner_root)
        });
        inspector.detach();
        wait_until("native Inspector detached", || {
            !inspector.is_attached()
                && frontend.is_mapped()
                && frontend.root().is_some_and(|root| root != owner_root)
        });
        let previous_closed = closed.get();
        gtk::prelude::WidgetExt::activate_action(&browser.window, "win.inspector", None).unwrap();
        eprintln!(
            "Inspector close cycle {cycle}: before={previous_closed}, after={}, mapped={}, attached={}",
            closed.get(),
            frontend.is_mapped(),
            inspector.is_attached()
        );
        wait_until("native Inspector toggle closes", || {
            closed.get() > previous_closed
                && !frontend.is_mapped()
                && frontend.root().is_none()
                && inspector.web_view().is_none()
        });
    }
    // Closing an inspected tab must close its tools without closing the other
    // tab or opening tools for a different tab.
    browser.add_tab(None, Some(&format!("{}/two", site.base)));
    let second = browser.active_tab().unwrap();
    wait_until("second Inspector owner fixture", || {
        second.view.title().as_deref() == Some("Two") && !second.view.is_loading()
    });
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.inspector", None).unwrap();
    let second_inspector = second.view.inspector().unwrap();
    let second_frontend = mapped_inspector(&second_inspector);
    second_inspector.detach();
    wait_until("tab-close Inspector detached", || {
        !second_inspector.is_attached()
            && second_frontend.is_mapped()
            && second_frontend
                .root()
                .is_some_and(|root| root != owner_root)
    });
    assert!(!inspector.web_view().is_some_and(|view| view.is_mapped()));
    browser.close_tab(second.id);
    wait_until("closed tab Inspector hidden", || {
        !second_frontend.is_mapped()
    });
    assert_eq!(browser.active_tab().unwrap().id, first.id);
    gtk::prelude::WidgetExt::activate_action(&browser.window, "win.inspector", None).unwrap();
    let frontend = mapped_inspector(&inspector);
    inspector.detach();
    wait_until("owner-close Inspector detached", || {
        !inspector.is_attached()
            && frontend.is_mapped()
            && frontend.root().is_some_and(|root| root != owner_root)
    });
    let previous_closed = closed.get();
    // Keep test references alive: owner-close must close the tools explicitly,
    // rather than depending on the eventual final release of the WebView.
    browser.window.close();
    wait_until("owner-close Inspector hidden", || {
        !frontend.is_mapped() && closed.get() > previous_closed
    });
    assert!(
        !base.exists(),
        "private Inspector fixture must not persist a profile"
    );
}

fn check_restart_child(app: &gtk::Application, stage: &str) {
    let base = std::path::PathBuf::from(std::env::var_os("PAISLEY_RESTART_ROOT").unwrap());
    assert!(base.is_absolute() && base.parent() == Some(std::env::temp_dir().as_path()));
    assert!(
        base.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("paisley-process-restart-")
    );
    let uri = std::env::var("PAISLEY_RESTART_URI").unwrap();
    let parsed = url::Url::parse(&uri).unwrap();
    assert_eq!(parsed.scheme(), "http");
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    assert!(parsed.port().is_some());
    assert!(matches!(stage, "seed" | "check" | "isolation"));
    let id = if stage == "isolation" {
        "other"
    } else {
        "owned"
    };
    let mut profile = paisley::profile::Profile::open_at(&base, id, false).unwrap();
    profile.settings.homepage = uri;
    profile.settings.restore_session = false;
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    let tab = browser.active_tab().unwrap();
    wait_until("separate-process fixture loaded", || {
        !tab.view.is_loading() && tab.view.title().as_deref() == Some("One")
    });
    if stage == "seed" {
        assert_eq!(
            javascript(
                &tab.view,
                "localStorage.setItem('paisley-profile','work'); document.cookie='paisley=work; max-age=3600; path=/'; localStorage.getItem('paisley-profile')"
            ),
            "work"
        );
    } else if stage == "isolation" {
        assert_eq!(
            javascript(&tab.view, "localStorage.getItem('paisley-profile')"),
            "null"
        );
        assert!(
            !javascript(&tab.view, "document.cookie").contains("paisley=work"),
            "another profile must not see the seeded cookie"
        );
    } else {
        assert_eq!(
            javascript(&tab.view, "localStorage.getItem('paisley-profile')"),
            "work",
            "owned value must survive actual process exit/relaunch"
        );
        assert!(javascript(&tab.view, "document.cookie").contains("paisley=work"));
    }
    browser.window.close();
    drop(tab);
    drop(browser);
    while gtk::glib::MainContext::default().pending() {
        gtk::glib::MainContext::default().iteration(false);
    }
}

fn check_process_restarts(site: &LocalSite) {
    let executable = std::env::current_exe().unwrap();
    for cycle in 1..=4 {
        let base = std::env::temp_dir().join(format!(
            "paisley-process-restart-{}-{cycle}",
            std::process::id()
        ));
        assert!(!base.exists());
        for stage in ["seed", "check", "isolation"] {
            eprintln!("separate-process restart {cycle}/4: {stage}");
            let output = std::process::Command::new(&executable)
                .args([
                    "browser_smoke",
                    "--ignored",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .env_remove("PAISLEY_RESTART_STRESS")
                .env_remove("PAISLEY_RESTART_PROCESSES")
                .env("PAISLEY_RESTART_CHILD", stage)
                .env("PAISLEY_RESTART_ROOT", &base)
                .env("PAISLEY_RESTART_URI", format!("{}/one", site.base))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "separate-process restart {cycle} {stage} failed: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        // Catalog removal is intentionally after each profile's actual GTK/WebKit
        // child process has exited; in-process teardown uses the deferred fixture
        // cleanup helper to avoid deleting storage under shared native watchers.
        let catalog = paisley::profile_catalog::ProfileCatalog::at(&base, false);
        catalog.delete("owned", true).unwrap();
        assert!(!base.join("owned").exists());
        std::fs::remove_dir_all(base).unwrap();
    }
}

fn check_persistent_profiles(app: &gtk::Application, site: &LocalSite, cycle: usize) {
    use paisley::profile::{Profile, Session};
    // Each stress cycle needs a new root: native asynchronous work can recreate
    // an earlier root after cleanup. Reusing its path is not a fresh-profile test.
    let base = std::env::temp_dir().join(format!("paisley-restart-{}-{cycle}", std::process::id()));
    assert!(!base.exists(), "test directory must be fresh");
    let profile = Profile::open_at(&base, "work", false).unwrap();
    profile
        .save_session(&Session {
            tabs: vec![format!("{}/one", site.base), format!("{}/two", site.base)],
            active: 0,
        })
        .unwrap();
    let browser = Browser::with_profile(app, profile).unwrap();
    browser.present();
    assert!(
        !browser.session.is_ephemeral(),
        "normal profiles must persist"
    );
    assert_eq!(browser.notebook.n_pages(), 2, "restore saved tabs");
    let first = browser.active_tab().unwrap();
    wait_until("restored active page", || {
        first.view.title().as_deref() == Some("One") && !first.view.is_loading()
    });
    browser.notebook.set_current_page(Some(1));
    let second = browser.active_tab().unwrap();
    wait_until("lazy restored background page", || {
        second.view.title().as_deref() == Some("Two") && !second.view.is_loading()
    });
    browser.notebook.reorder_child(&second.page, Some(0));
    javascript(
        &second.view,
        "localStorage.setItem('paisley-profile', 'work'); document.cookie='paisley=work; max-age=3600; path=/'; 'stored'",
    );
    let native_view = second.view.downgrade();
    let native_session = browser.session.downgrade();
    browser.window.close();
    drop(first);
    drop(second);
    drop(browser);
    // Pump cleanup timers so the controller and profile lifetime end.
    while gtk::glib::MainContext::default().pending() {
        gtk::glib::MainContext::default().iteration(false);
    }
    if std::env::var_os("PAISLEY_RESTART_STRESS").is_some() {
        eprintln!(
            "restart native lifetime: view_alive={} session_alive={}",
            native_view.upgrade().is_some(),
            native_session.upgrade().is_some()
        );
    }
    let profile = Profile::open_at(&base, "work", false).unwrap();
    let saved = profile.load_session().unwrap();
    assert_eq!(
        saved.tabs,
        vec![format!("{}/two", site.base), format!("{}/one", site.base)]
    );
    assert_eq!(saved.active, 0);
    let restarted = Browser::with_profile(app, profile).unwrap();
    restarted.present();
    let tab = restarted.active_tab().unwrap();
    wait_until("persistent site data after restart", || {
        !tab.view.is_loading() && tab.view.title().as_deref() == Some("Two")
    });
    let restored = javascript(&tab.view, "localStorage.getItem('paisley-profile')");
    if restored != "work" {
        // Read only the disposable fixture's SQLite state after observing a
        // failure. No profile roots from the host enter the isolated runner.
        let diagnostic = std::process::Command::new("/usr/bin/python3").args(["-I", "-c", r#"
import itertools, pathlib, sqlite3, sys
root = pathlib.Path(sys.argv[1])
for path in itertools.islice(root.rglob('*'), 128):
    if path.is_symlink() or not path.is_file():
        continue
    with path.open('rb') as file:
        header = file.read(16)
    print('owned fixture file:', path.relative_to(root), 'bytes=', path.stat().st_size, 'sqlite=', header == b'SQLite format 3\0')
    if header != b'SQLite format 3\0':
        continue
    try:
        connection = sqlite3.connect(path.as_uri() + '?mode=ro', uri=True, timeout=0.1)
        try:
            tables = [row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table' LIMIT 16")]
            if 'ItemTable' in tables:
                row = connection.execute('SELECT value FROM ItemTable WHERE key = ?', ('paisley-profile',)).fetchone()
                print('owned storage diagnostic:', path.relative_to(root), 'fixture-row-present=', row is not None, 'fixture-value-matches=', bool(row and row[0] in ('work', b'work', 'work'.encode('utf-16-le'))))
        finally:
            connection.close()
    except sqlite3.Error:
        print('owned storage diagnostic:', path.relative_to(root), 'SQLite read unavailable')
"#]).arg(&base).output();
        match diagnostic {
            Ok(output) => {
                eprintln!("{}", String::from_utf8_lossy(&output.stdout));
                eprintln!("diagnostic exit: {}", output.status);
            }
            Err(error) => eprintln!("Optional storage diagnostic unavailable: {error}"),
        }
    }
    assert_eq!(restored, "work");
    assert!(javascript(&tab.view, "document.cookie").contains("paisley=work"));
    restarted.window.close();
    drop(tab);
    drop(restarted);
    let mut other = Profile::open_at(&base, "personal", false).unwrap();
    assert!(other.load_session().unwrap().tabs.is_empty());
    other.settings.homepage = format!("{}/one", site.base);
    let isolated = Browser::with_profile(app, other).unwrap();
    isolated.present();
    let tab = isolated.active_tab().unwrap();
    wait_until("isolated profile page", || {
        !tab.view.is_loading() && tab.view.title().as_deref() == Some("One")
    });
    assert_eq!(
        javascript(
            &tab.view,
            "localStorage.getItem('paisley-profile') === null && document.cookie === ''"
        ),
        "true"
    );
    isolated.window.close();
    drop(tab);
    drop(isolated);
    defer_engine_fixture_cleanup_until_process_exit(&base);
}

fn save_bookmark_draft(browser: &Rc<Browser>) {
    library_widgets(browser.window.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| b.label().as_deref() == Some("Save bookmark"))
        .expect("bookmark draft save")
        .emit_clicked();
    browser.menu.dismiss();
}

fn library_sheet_panel(browser: &Rc<Browser>) -> gtk::Widget {
    library_widgets(browser.window.upcast_ref())
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Stack>().ok())
        .find_map(|stack| stack.child_by_name("secondary"))
        .expect("secondary library sheet")
}

// Closed tabs must release their native widget and content-manager ownership;
// engine allocator/cache retention is a separate measurement.
fn check_closed_tab_lifetime(app: &gtk::Application, site: &LocalSite) {
    let browser = Browser::new(app);
    browser.present();
    for cycle in 0..12 {
        let view = browser.add_tab(None, Some(&format!("{}/one?lifetime={cycle}", site.base)));
        wait_until("lifetime page loaded", || !view.is_loading() && view.title().is_some());
        let id = browser.active_tab().unwrap().id;
        let weak_view = view.downgrade();
        let weak_manager = view.user_content_manager().unwrap().downgrade();
        drop(view);
        browser.close_tab(id);
        wait_until("closed tab WebView released", || weak_view.upgrade().is_none());
        wait_until("closed tab content manager released", || weak_manager.upgrade().is_none());
        assert_eq!(browser.notebook.n_pages(), 1);
        eprintln!("closed-tab ownership cycle {}/12 released", cycle + 1);
    }
    browser.window.close();
}
