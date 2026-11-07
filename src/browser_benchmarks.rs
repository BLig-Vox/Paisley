// Opt-in real-browser measurement on the existing single GTK test thread.
// This module is test-only and never enables a privileged production mode.
use super::*;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
struct Summary {
    count: usize,
    min_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

fn summarize(samples: &[f64]) -> Option<Summary> {
    if samples.is_empty()
        || samples
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return None;
    }
    let mut values = samples.to_vec();
    values.sort_by(f64::total_cmp);
    let rank = |percent: usize| values[(values.len() * percent).div_ceil(100).saturating_sub(1)];
    Some(Summary {
        count: values.len(),
        min_ms: values[0],
        p50_ms: rank(50),
        p95_ms: rank(95),
        max_ms: values[values.len() - 1],
    })
}

fn parse_cpu(stat: &str) -> Option<(u32, u64, u64)> {
    let (_, fields) = stat.rsplit_once(')')?;
    let fields: Vec<_> = fields.split_whitespace().collect();
    let parent = fields.get(1)?.parse().ok()?;
    let user: u64 = fields.get(11)?.parse().ok()?;
    let system: u64 = fields.get(12)?.parse().ok()?;
    Some((
        parent,
        fields.get(19)?.parse().ok()?,
        user.checked_add(system)?,
    ))
}

#[test]
fn benchmark_cpu_parser_handles_parentheses_and_ignores_child_cpu_totals() {
    assert_eq!(
        parse_cpu("42 (WebKit (child)) S 10 0 0 0 0 0 0 0 0 0 7 11 900 800 0 0 0 0 123456 0"),
        Some((10, 123456, 18))
    );
    assert_eq!(parse_cpu("broken"), None);
}

fn background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    let mut value = None;
    wait_until("background benchmark sample", || {
        value = rx.try_recv().ok();
        value.is_some()
    });
    worker.join().expect("benchmark sampler");
    value.unwrap()
}

fn cpu_tree() -> std::collections::HashMap<(u32, u64), u64> {
    use std::collections::{HashMap, HashSet};
    let mut rows = HashMap::new();
    for entry in std::fs::read_dir("/proc").unwrap().take(65536).flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if let Ok(text) = std::fs::read_to_string(entry.path().join("stat"))
            && let Some(row) = parse_cpu(&text)
        {
            rows.insert(pid, row);
        }
    }
    let mut owned = HashSet::from([std::process::id()]);
    loop {
        let previous = owned.len();
        for (&pid, &(parent, _, _)) in &rows {
            if owned.contains(&parent) {
                owned.insert(pid);
            }
        }
        if previous == owned.len() {
            break;
        }
    }
    rows.into_iter()
        .filter(|(pid, _)| owned.contains(pid))
        .map(|(pid, (_, start, ticks))| ((pid, start), ticks))
        .collect()
}

fn ready(browser: &Browser, expected: &str) {
    let started = Instant::now();
    let mut reported = false;
    wait_until("benchmark fixture ready", || {
        if !reported && started.elapsed() > Duration::from_secs(19) {
            reported = true;
            eprintln!(
                "BENCHMARK wait: expected={expected}, tabs={}, current={:?}, mapped={}, filter_ready={}, active={:?}",
                browser.tabs.borrow().len(),
                browser.notebook.current_page(),
                browser.window.is_mapped(),
                browser.blocker.ready.get(),
                browser.active_tab().map(|tab| (
                    tab.view.uri(),
                    tab.view.title(),
                    tab.view.is_loading(),
                    tab.view.width(),
                    tab.view.height()
                ))
            );
        }
        browser.blocker.ready.get()
            && browser.window.is_mapped()
            && browser.active_tab().is_some_and(|tab| {
                tab.view.uri().as_deref() == Some(expected)
                    && !tab.view.is_loading()
                    && tab.view.width() > 0
                    && tab.view.height() > 0
                    && tab.view.title().is_some()
            })
    });
    assert!(
        browser.blocker.error.borrow().is_none(),
        "benchmark filter compilation failed"
    );
}

fn open_browser(
    app: &gtk::Application,
    base: &Path,
    id: &str,
    home: &str,
    fresh: bool,
) -> Rc<Browser> {
    let mut profile = browny::profile::Profile::open_at(base, id, false).unwrap();
    if fresh {
        profile.settings.homepage = home.into();
        profile.save_settings(&profile.settings).unwrap();
    }
    let browser = Browser::with_profile(app, profile).unwrap();
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(|_, _, _, _| gtk::glib::Propagation::Stop);
    browser.window.add_controller(keys);
    browser.present();
    ready(&browser, home);
    browser
}

fn close_browser(browser: Rc<Browser>) {
    browser.window.close();
    drop(browser);
    let context = gtk::glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
}

struct Evidence {
    output: std::path::PathBuf,
    directory: std::path::PathBuf,
}

impl Evidence {
    fn new(output: &Path) -> Self {
        use std::os::unix::fs::DirBuilderExt;
        assert!(
            output.symlink_metadata().is_err(),
            "benchmark output must not exist"
        );
        let directory = output.parent().unwrap().join(format!(
            "benchmark-profiles-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        Self {
            output: output.to_owned(),
            directory,
        }
    }

    fn publish(&self, report: &serde_json::Value) {
        use std::os::unix::fs::OpenOptionsExt;
        let mut bytes = serde_json::to_vec_pretty(report).unwrap();
        bytes.push(b'\n');
        assert!(bytes.len() <= 1024 * 1024, "bounded benchmark JSON");
        let staged = self.directory.join("completed-report.json");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&staged)
            .unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
        std::fs::hard_link(&staged, &self.output)
            .expect("publish complete report without overwriting");
    }
}

impl Drop for Evidence {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.directory) {
            eprintln!("benchmark scratch cleanup failed: {error}");
        }
    }
}

struct BenchmarkWindows<'a>(&'a gtk::Application);
impl Drop for BenchmarkWindows<'_> {
    fn drop(&mut self) {
        for window in self.0.windows() {
            window.close();
        }
        let context = gtk::glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
    }
}

#[test]
fn benchmark_failure_removes_profiles_without_publishing_an_empty_report() {
    let root = std::env::temp_dir().join(format!(
        "browny-evidence-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let output = root.join("report.json");
    let _ = std::panic::catch_unwind(|| {
        let evidence = Evidence::new(&output);
        std::fs::write(
            evidence.directory.join("fixture-record"),
            b"owned temporary profile",
        )
        .unwrap();
        panic!("simulated benchmark failure");
    });
    let published = output.exists();
    let remaining = std::fs::read_dir(&root).unwrap().count();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(!published, "failed measurement published an empty report");
    assert_eq!(remaining, 0, "failed measurement retained profile data");
}

fn next_selection(current: u32, count: u32) -> (u32, &'static str) {
    assert!(count > 0 && current < count);
    (
        (current + 1) % count,
        if count == 1 {
            "no_op_baseline"
        } else {
            "different_tab_dispatch"
        },
    )
}

#[test]
fn benchmark_selection_labels_noop_and_never_reselects_current_in_multitab_run() {
    assert_eq!(next_selection(0, 1), (0, "no_op_baseline"));
    for count in [10, 30] {
        for current in 0..count {
            let (next, kind) = next_selection(current, count);
            assert_ne!(next, current);
            assert!(next < count);
            assert_eq!(kind, "different_tab_dispatch");
        }
    }
}

pub(super) fn run(app: &gtk::Application, site: &LocalSite, output: &Path) {
    use serde_json::json;
    use std::fs::OpenOptions;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    if cfg!(debug_assertions) {
        panic!("use cargo test --release for performance measurements");
    }
    assert!(
        output.is_absolute(),
        "benchmark output must be a new absolute JSON path"
    );
    let evidence = Evidence::new(output);
    let _windows = BenchmarkWindows(app);
    let scratch = evidence.directory.join("profiles");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&scratch)
        .unwrap();
    let home = format!("{}/one", site.base);
    let mut fresh_ms = Vec::new();
    let mut restored_ms = Vec::new();
    for run in 0..5 {
        let id = format!("startup-{run}");
        let started = Instant::now();
        let browser = open_browser(app, &scratch, &id, &home, true);
        fresh_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        close_browser(browser);
        let started = Instant::now();
        let browser = open_browser(app, &scratch, &id, &home, false);
        restored_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        close_browser(browser);
    }
    let browser = open_browser(app, &scratch, "tabs", &home, true);
    let renderer = browser
        .window
        .renderer()
        .map(|renderer| renderer.type_().name().to_string());
    let mut capture_ms = Vec::new();
    let mut png_ms = Vec::new();
    let mut capture_size = None;
    for capture_index in 0..10 {
        let view = browser.active_tab().unwrap().view;
        let result = Rc::new(RefCell::new(None));
        let callback = Rc::clone(&result);
        let started = Instant::now();
        view.snapshot(
            webkit::SnapshotRegion::Visible,
            webkit::SnapshotOptions::NONE,
            None::<&gtk::gio::Cancellable>,
            move |texture| *callback.borrow_mut() = Some(texture),
        );
        wait_until("native benchmark snapshot", || result.borrow().is_some());
        let texture = result
            .borrow_mut()
            .take()
            .unwrap()
            .expect("real WebKit snapshot");
        capture_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(texture.width() >= 640 && texture.height() >= 300);
        let started = Instant::now();
        let png = texture.save_to_png_bytes();
        png_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(png.as_ref().starts_with(b"\x89PNG\r\n\x1a\n"));
        if capture_index == 9
            && let Some(path) = std::env::var_os("BROWNY_BENCHMARK_PAGE_SCREENSHOT")
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .unwrap();
            file.write_all(png.as_ref()).unwrap();
        }
        capture_size =
            Some(json!({"width":texture.width(),"height":texture.height(),"png_bytes":png.len()}));
    }
    let before = background(cpu_tree);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        let context = gtk::glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let after = background(cpu_tree);
    let elapsed = started.elapsed().as_secs_f64();
    let ticks: u64 = after
        .iter()
        .filter_map(|(key, value)| before.get(key).map(|old| value.saturating_sub(*old)))
        .sum();
    let clock = std::process::Command::new("/usr/bin/getconf")
        .arg("CLK_TCK")
        .output()
        .expect("installed POSIX getconf");
    assert!(clock.status.success());
    let hz: f64 = String::from_utf8(clock.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(hz.is_finite() && hz > 0.0);
    let idle_percent = ticks as f64 / hz / elapsed * 100.0;
    if let Some(path) = std::env::var_os("BROWNY_BENCHMARK_SCREENSHOT") {
        assert!(
            std::fs::symlink_metadata(path).is_err(),
            "choose a new benchmark window PNG path"
        );
    }
    capture_native_window(browser.window.upcast_ref(), "BROWNY_BENCHMARK_SCREENSHOT");
    let mut tabs = Vec::new();
    for count in [1usize, 10, 30] {
        while browser.tabs.borrow().len() < count {
            browser.add_tab(None, Some(&home));
            ready(&browser, &home);
        }
        wait_until("all benchmark tabs loaded", || {
            browser
                .tabs
                .borrow()
                .iter()
                .all(|tab| !tab.view.is_loading() && tab.view.title().as_deref() == Some("One"))
        });
        let mut selection_ms = Vec::new();
        for _ in 0..64 {
            let (index, _) = next_selection(browser.notebook.current_page().unwrap(), count as u32);
            let started = Instant::now();
            browser.notebook.set_current_page(Some(index));
            let context = gtk::glib::MainContext::default();
            while context.pending() && started.elapsed() < Duration::from_secs(2) {
                context.iteration(false);
            }
            assert_eq!(browser.notebook.current_page(), Some(index));
            selection_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let usage = background(|| browny::memory::sample(std::process::id()));
        let memory = usage.map(|usage| json!({"kib":usage.kib,"metric":usage.metric,"processes":usage.processes,"partial":usage.partial}));
        tabs.push(json!({"loaded_tabs":count,"selection_kind":next_selection(0, count as u32).1,"selection_dispatch_ms":summarize(&selection_ms),"samples_ms":selection_ms,"memory":memory}));
    }
    let mut blocking = Vec::new();
    for mode in ["disabled", "starter_nonmatching", "fixture_matching"] {
        if mode == "fixture_matching" {
            browser.blocker.compile(
                r#"[{"trigger":{"url-filter":"/tracker\\.js"},"action":{"type":"block"}}]"#,
                || {},
            );
            wait_until("benchmark matching filter", || browser.blocker.ready.get());
        }
        browser.settings.borrow_mut().block_trackers = mode != "disabled";
        browser.apply_settings();
        let before = site.trackers.load(Ordering::SeqCst);
        let mut navigation_ms = Vec::new();
        for iteration in 0..10 {
            let uri = format!("{}/blocking?benchmark={mode}-{iteration}", site.base);
            let started = Instant::now();
            browser.navigate(&uri);
            ready(&browser, &uri);
            navigation_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let received = site.trackers.load(Ordering::SeqCst) - before;
        assert_eq!(
            received,
            if mode == "fixture_matching" { 0 } else { 10 },
            "filter fixture request count"
        );
        blocking.push(json!({"mode":mode,"navigation_finished_ms":summarize(&navigation_ms),"samples_ms":navigation_ms,"tracker_requests":received}));
    }
    let cpu_model = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            line.strip_prefix("model name")
                .and_then(|value| value.split_once(':'))
                .map(|(_, value)| value.trim().to_string())
        });
    let mem_total = std::fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("MemTotal:"))
        .map(str::to_owned);
    let view = browser.active_tab().unwrap().view;
    let report = json!({
        "schema_version":1,"crate_version":env!("CARGO_PKG_VERSION"),"release":true,
        "environment":{"gtk":format!("{}.{}.{}", gtk::major_version(),gtk::minor_version(),gtk::micro_version()),"webkit":format!("{}.{}.{}",webkit::functions::major_version(),webkit::functions::minor_version(),webkit::functions::micro_version()),"kernel":std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim(),"cpu_model":cpu_model,"memory_total":mem_total,"renderer":renderer,"scale":browser.window.scale_factor(),"viewport":{"width":view.width(),"height":view.height()},"clock_ticks_per_second":hz},
        "startup_in_process_ms":{"fresh_profile":summarize(&fresh_ms),"restored_profile":summarize(&restored_ms),"fresh_samples":fresh_ms,"restored_samples":restored_ms},
        "capture":{"native_snapshot_ms":summarize(&capture_ms),"png_encode_ms":summarize(&png_ms),"snapshot_samples_ms":capture_ms,"png_samples_ms":png_ms,"image":capture_size},
        "idle":{"seconds":elapsed,"owned_cpu_percent_of_one_core":idle_percent,"partial":true,"method":"same PID/start-time processes present at both samples; excludes exited/new processes; includes GTK test/HTTP fixture overhead"},
        "tabs":tabs,"blocking":blocking,
        "limitations":["in-process GTK test, not standalone launch or cold disk-cache startup","fixture is a tiny loopback document; no public compatibility claim","memory and CPU ownership are Linux-visible and partial, not all profile runtimes","selection measures chrome/main-context dispatch, not compositor-presented page paint","idle includes fixture/test harness and cannot isolate incremental monitoring CPU","small nonrandomized same-session samples, not production budgets or browser comparisons"]
    });
    close_browser(browser);
    std::fs::remove_dir_all(&scratch).unwrap();
    evidence.publish(&report);
    println!(
        "BENCHMARK: wrote owned-fixture release measurements to {}",
        output.display()
    );
}

#[test]
fn benchmark_percentiles_use_nearest_rank_without_dropping_outliers() {
    let result = summarize(&[100.0, 20.0, 10.0, 40.0, 30.0]).unwrap();
    assert_eq!(result.count, 5);
    assert_eq!(result.min_ms, 10.0);
    assert_eq!(result.p50_ms, 30.0);
    assert_eq!(result.p95_ms, 100.0);
    assert_eq!(result.max_ms, 100.0);
    let single = summarize(&[0.0]).unwrap();
    assert_eq!(single.p50_ms, 0.0);
    assert_eq!(single.p95_ms, 0.0);
}

#[test]
fn benchmark_statistics_reject_empty_negative_or_nonfinite_timings() {
    for values in [
        &[][..],
        &[f64::NAN],
        &[f64::INFINITY],
        &[-1.0],
        &[1.0, f64::NEG_INFINITY],
    ] {
        assert!(summarize(values).is_none());
    }
}
