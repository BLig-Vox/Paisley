use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Process {
    parent: u32,
    rss_kib: u64,
}

fn parse_status(text: &str) -> Option<Process> {
    let parent = field(text, "PPid:")?.parse().ok()?;
    let rss_kib = field(text, "VmRSS:")?.parse().ok()?;
    Some(Process { parent, rss_kib })
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.split_whitespace().next())
}

fn descendants(root: u32, processes: &HashMap<u32, Process>) -> HashSet<u32> {
    let mut owned = HashSet::new();
    if !processes.contains_key(&root) {
        return owned;
    }
    owned.insert(root);
    loop {
        let previous = owned.len();
        for (&pid, process) in processes {
            if owned.contains(&process.parent) {
                owned.insert(pid);
            }
        }
        if owned.len() == previous {
            return owned;
        }
    }
}

/// Linux-visible owned process tree, not all WebKit processes on the desktop.
/// Process namespaces/reparenting may hide engine children. Never call on GTK's
/// main thread; /proc and smaps reads can block.
#[derive(Clone, Debug)]
pub struct Usage {
    pub kib: u64,
    pub processes: usize,
    pub metric: &'static str,
    pub partial: bool,
}

pub fn sample(root: u32) -> Option<Usage> {
    let mut processes = HashMap::new();
    for entry in std::fs::read_dir("/proc").ok()?.take(65536).flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if let Ok(status) = std::fs::read_to_string(entry.path().join("status"))
            && let Some(process) = parse_status(&status)
        {
            processes.insert(pid, process);
        }
    }
    let owned = descendants(root, &processes);
    if owned.is_empty() {
        return None;
    }
    let rss: u64 = owned.iter().map(|pid| processes[pid].rss_kib).sum();
    let pss: Option<u64> = owned.iter().try_fold(0u64, |sum, pid| {
        let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
        let value: u64 = field(&text, "Pss:")?.parse().ok()?;
        sum.checked_add(value)
    });
    Some(Usage {
        kib: pss.unwrap_or(rss),
        processes: owned.len(),
        metric: if pss.is_some() { "PSS" } else { "RSS" },
        // Visible ancestry is not proof of full cgroup/namespace coverage.
        partial: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_memory_and_parent_without_relying_on_field_order() {
        assert_eq!(
            parse_status("Name:\tWebKit\nVmRSS:\t2048 kB\nPPid:\t42\n"),
            Some(Process {
                parent: 42,
                rss_kib: 2048
            })
        );
        assert_eq!(parse_status("PPid:\t42\nVmRSS:\tinvalid kB\n"), None);
        assert_eq!(parse_status("PPid:\t42\n"), None);
    }
    #[test]
    fn includes_owned_descendants_once_not_unrelated_engine_processes() {
        let p = |parent| Process {
            parent,
            rss_kib: 100,
        };
        let processes = HashMap::from([
            (10, p(1)),
            (11, p(10)),
            (12, p(11)),
            (90, p(1)),
            (91, p(90)),
        ]);
        assert_eq!(descendants(10, &processes), HashSet::from([10, 11, 12]));
        assert!(descendants(200, &processes).is_empty());
    }
}
