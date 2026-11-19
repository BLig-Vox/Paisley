//! No GTK/CEF linkage: prepare the runtime before the ELF loader opens the engine.
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    env,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::Command,
};

const REQUIRED: &[&str] = &[
    "libcef.so",
    "icudtl.dat",
    "resources.pak",
    "chrome_100_percent.pak",
    "chrome_200_percent.pak",
    "v8_context_snapshot.bin",
    "chrome-sandbox",
    "locales/en-US.pak",
];

fn complete_runtime(path: &Path) -> bool {
    path.is_absolute()
        && REQUIRED.iter().all(|file| path.join(file).is_file())
        && std::fs::read_to_string(path.join("include/cef_version.h")).is_ok_and(|header| {
            header.lines().any(|line| {
                line == "#define CEF_VERSION \"156.0.8+g426920c+chromium-156.0.8078.48\""
            })
        })
}

fn resolve_runtime(path: &Path) -> Option<PathBuf> {
    // Accept the exact distribution and cef-rs's documented versioned parent.
    for candidate in [
        path.to_path_buf(),
        path.join("cef_linux_x86_64"),
        path.join("156.0.8/cef_linux_x86_64"),
    ] {
        if complete_runtime(&candidate) {
            return candidate.canonicalize().ok();
        }
    }
    None
}

fn runtime(executable: &Path) -> io::Result<PathBuf> {
    // Explicit configuration must fail visibly rather than silently select another engine.
    for name in ["PAISLEY_CEF_RUNTIME", "CEF_PATH"] {
        if let Some(path) = env::var_os(name) {
            return resolve_runtime(Path::new(&path)).ok_or_else(|| {
                io::Error::other(format!(
                    "{name} does not name a complete absolute CEF 156.0.8 runtime directory"
                ))
            });
        }
    }
    let mut candidates = vec![executable.parent().unwrap().join("../lib/paisley/cef")];
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if let Some(data) = data {
        candidates.push(data.join("paisley/cef"));
    }
    if let Some(path) = option_env!("CEF_PATH") {
        candidates.push(PathBuf::from(path));
    }
    candidates.into_iter().find_map(|p| resolve_runtime(&p)).ok_or_else(|| io::Error::other(
        "CEF runtime not found. Install the complete matching CEF 156.0.8 distribution, then set PAISLEY_CEF_RUNTIME to its absolute directory (see docs/CEF_PRERELEASE.md). No WebKit fallback was started."))
}

fn prepare_engine(
    executable: &Path,
    args: impl IntoIterator<Item = OsString>,
) -> io::Result<Command> {
    let engine = executable.with_file_name("paisley-engine");
    if !engine.is_file() {
        return Err(io::Error::other(
            "Missing sibling paisley-engine. Reinstall both binaries with cargo install --path . --locked (without --bin).",
        ));
    }
    let mut command = Command::new(engine);
    command.args(args);
    if cfg!(feature = "cef") {
        let runtime = runtime(executable)?;
        let mut paths = vec![runtime.clone()];
        if let Some(old) = env::var_os("LD_LIBRARY_PATH") {
            paths.extend(env::split_paths(&old));
        }
        command.env(
            "LD_LIBRARY_PATH",
            env::join_paths(paths).map_err(io::Error::other)?,
        );
        command.env("PAISLEY_CEF_RUNTIME", runtime);
    }
    Ok(command)
}

fn run() -> io::Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!(
            "Paisley\nUsage: paisley [--profile ID] [--private] [https://URL ...]\n       paisley --install-desktop\n       paisley --backend-info\nBackend: {}. Normal profiles persist site data; private windows do not.\nCEF requires its matching complete runtime; see docs/CEF_PRERELEASE.md.",
            if cfg!(feature = "cef") {
                "CEF"
            } else {
                "WebKit (deprecated)"
            }
        );
        return Ok(());
    }
    if args.len() == 1 && args[0] == "--backend-info" {
        println!(
            "Backend: {}",
            if cfg!(feature = "cef") {
                "CEF 156.0.8"
            } else {
                "WebKit (deprecated)"
            }
        );
        if cfg!(feature = "cef") {
            println!("Runtime: {}", runtime(&env::current_exe()?)?.display());
        }
        return Ok(());
    }
    let mut command = prepare_engine(&env::current_exe()?, args)?;
    #[cfg(unix)]
    return Err(command.exec());
    #[cfg(not(unix))]
    {
        std::process::exit(command.status()?.code().unwrap_or(1));
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Paisley: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = env::temp_dir().join(format!(
                "paisley-launcher-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn populate(&self, path: &Path) {
            std::fs::create_dir_all(path.join("include")).unwrap();
            std::fs::write(
                path.join("include/cef_version.h"),
                "#define CEF_VERSION \"156.0.8+g426920c+chromium-156.0.8078.48\"\n",
            )
            .unwrap();
            for file in REQUIRED {
                let file = path.join(file);
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(file, b"fixture").unwrap();
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn rejects_library_only_and_relative_runtime() {
        let f = Fixture::new();
        std::fs::write(f.0.join("libcef.so"), b"fixture").unwrap();
        assert!(resolve_runtime(&f.0).is_none());
        assert!(resolve_runtime(Path::new("relative")).is_none());
    }
    #[test]
    fn discovers_exact_and_versioned_parent() {
        let f = Fixture::new();
        let child = f.0.join("156.0.8/cef_linux_x86_64");
        f.populate(&child);
        assert_eq!(
            resolve_runtime(&f.0).unwrap(),
            child.canonicalize().unwrap()
        );
        assert_eq!(
            resolve_runtime(&child).unwrap(),
            child.canonicalize().unwrap()
        );
        std::fs::remove_file(child.join("locales/en-US.pak")).unwrap();
        assert!(resolve_runtime(&f.0).is_none());
    }
    #[test]
    fn missing_engine_is_an_actionable_error() {
        let f = Fixture::new();
        let error = prepare_engine(&f.0.join("paisley"), []).unwrap_err();
        assert!(error.to_string().contains("Missing sibling paisley-engine"));
    }
}
