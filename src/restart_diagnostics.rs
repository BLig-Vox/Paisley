// Test-only, passive process-exit diagnostics for the private fixture namespace.
#[derive(Debug, PartialEq)]
struct ProcessSample {
    name: String,
    state: char,
    started: u64,
    raw_status: i32,
}

fn parse_stat(stat: &str) -> Option<ProcessSample> {
    let (_, rest) = stat.split_once('(')?;
    let (name, fields) = rest.rsplit_once(')')?;
    let fields: Vec<_> = fields.split_whitespace().collect();
    let state = fields.first()?;
    if state.len() != 1 {
        return None;
    }
    Some(ProcessSample {
        name: name.to_owned(),
        state: state.chars().next()?,
        started: fields.get(19)?.parse().ok()?,
        raw_status: fields.get(49)?.parse().ok()?,
    })
}

fn scope_allowed(explicit_fixture: bool, pid_one_name: &str) -> bool {
    explicit_fixture && pid_one_name.trim() == "bwrap"
}

fn read_sample(pid: u32) -> Option<ProcessSample> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(format!("/proc/{pid}/stat"))
        .ok()?
        .take(8192)
        .read_to_string(&mut text)
        .ok()?;
    parse_stat(&text)
}

pub(super) struct NetworkExitMonitor {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl NetworkExitMonitor {
    pub(super) fn start() -> Option<Self> {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let pid_one = std::fs::read_to_string("/proc/1/comm").unwrap_or_default();
        if !scope_allowed(
            std::env::var_os("BROWNY_DOWNLOAD_CAPACITY_FIXTURE").is_some(),
            &pid_one,
        ) {
            eprintln!("owned network exit monitor unavailable: not a private fixture namespace");
            return None;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let finished = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            use std::os::unix::process::ExitStatusExt;
            let mut known = std::collections::HashMap::<u32, (u64, bool)>::new();
            let mut events = 0;
            eprintln!(
                "owned network exit monitor: passive 1ms samples, at most 32 events; rapid reaping may hide status"
            );
            while !finished.load(Ordering::Relaxed) && events < 32 {
                if let Ok(entries) = std::fs::read_dir("/proc") {
                    for entry in entries.take(4096).flatten() {
                        if events >= 32 {
                            break;
                        }
                        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                            continue;
                        };
                        let Some(sample) = read_sample(pid) else {
                            continue;
                        };
                        if sample.name != "WebKitNetworkPr" {
                            continue;
                        }
                        if known.len() < 32
                            && let std::collections::hash_map::Entry::Vacant(entry) =
                                known.entry(pid)
                        {
                            eprintln!(
                                "owned network observed: pid={pid} started={}",
                                sample.started
                            );
                            entry.insert((sample.started, false));
                            events += 1;
                        }
                    }
                }
                known.retain(|pid, (started, terminal_seen)| {
                    if events >= 32 { return true; }
                    let sample = read_sample(*pid).filter(|sample| sample.started == *started);
                    match sample {
                        Some(sample) if sample.state == 'Z' && !*terminal_seen => {
                            let status = std::process::ExitStatus::from_raw(sample.raw_status);
                            eprintln!("owned network terminal: pid={pid} started={started} raw_status={} signal={:?} exit_code={:?} core_dumped={}",
                                sample.raw_status, status.signal(), status.code(), status.core_dumped());
                            *terminal_seen = true;
                            events += 1;
                        }
                        None => {
                            if !*terminal_seen {
                                eprintln!("owned network vanished: pid={pid} started={started} exit_status=unobserved");
                                events += 1;
                            }
                            return false;
                        }
                        _ => {}
                    }
                    true
                });
                std::thread::park_timeout(std::time::Duration::from_millis(1));
            }
            eprintln!("owned network exit monitor stopped: events={events}");
        });
        Some(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for NetworkExitMonitor {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn stat(name: &str, state: char, started: u64, status: i32) -> String {
        let mut fields = vec!["0".to_owned(); 50];
        fields[0] = state.to_string();
        fields[19] = started.to_string();
        fields[49] = status.to_string();
        format!("42 ({name}) {}", fields.join(" "))
    }

    #[test]
    fn monitor_requires_explicit_fixture_and_private_pid_namespace() {
        assert!(!scope_allowed(false, "bwrap\n"));
        assert!(!scope_allowed(true, "systemd\n"));
        assert!(!scope_allowed(true, ""));
        assert!(scope_allowed(true, "bwrap\n"));
    }

    #[test]
    fn linux_exit_parser_preserves_identity_and_wait_status() {
        let sample = parse_stat(&stat("owned (fixture)", 'Z', 123, 9)).unwrap();
        assert_eq!(sample.name, "owned (fixture)");
        assert_eq!(sample.started, 123);
        assert_eq!(sample.state, 'Z');
        assert_eq!(
            std::process::ExitStatus::from_raw(sample.raw_status).signal(),
            Some(9)
        );
        let normal = parse_stat(&stat("fixture", 'Z', 124, 7 << 8)).unwrap();
        assert_eq!(
            std::process::ExitStatus::from_raw(normal.raw_status).code(),
            Some(7)
        );
    }

    #[test]
    fn linux_exit_parser_rejects_incomplete_and_malformed_samples() {
        for input in ["", "42 (fixture) Z", "42 fixture Z", "42 (fixture) ZZ"] {
            assert!(parse_stat(input).is_none());
        }
        let input = stat("fixture", 'S', 1, 0);
        assert_eq!(parse_stat(&input).unwrap().state, 'S');
        assert!(parse_stat(&input.replace(" S ", " SS ")).is_none());
    }

    #[test]
    fn owned_killed_child_exposes_real_linux_terminal_status_before_reap() {
        struct ChildGuard(std::process::Child);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = ChildGuard(
            std::process::Command::new("/usr/bin/sleep")
                .arg("30")
                .spawn()
                .unwrap(),
        );
        child.0.kill().unwrap();
        let path = format!("/proc/{}/stat", child.0.id());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let sample = loop {
            if let Some(sample) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| parse_stat(&text))
                .filter(|sample| sample.state == 'Z')
            {
                break sample;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "owned child never exposed terminal status"
            );
            std::thread::yield_now();
        };
        assert_eq!(
            std::process::ExitStatus::from_raw(sample.raw_status).signal(),
            Some(9)
        );
        assert_eq!(child.0.wait().unwrap().signal(), Some(9));
    }
}
