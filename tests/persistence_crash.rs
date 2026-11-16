//! Actual child-process interruption of owned profile metadata writes.
//! No GTK session, network, personal profile or production fault hook is used.
use paisley::profile::{Profile, Session};
use std::{
    fs::{self, File},
    io::Read,
    os::unix::{fs::DirBuilderExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const CHILD: &str = "PAISLEY_PERSISTENCE_CRASH_CHILD";
const ROOT: &str = "PAISLEY_PERSISTENCE_CRASH_ROOT";

fn snapshot(new: bool) -> Session {
    let padding = "x".repeat(1400);
    Session {
        tabs: (0..200)
            .map(|index| format!("https://fixture.invalid/{new}/{index}?{padding}"))
            .collect(),
        active: if new { 137 } else { 42 },
    }
}

// Ensure assertion/timeout unwinding cannot leave a background writer running.
struct Writer(Child);
impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(writer: &mut Writer, description: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if ready() {
            return;
        }
        assert!(
            writer.0.try_wait().unwrap().is_none(),
            "writer exited before {description}"
        );
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        std::thread::yield_now();
    }
}

fn child(stage: &str, root: &Path) {
    assert!(matches!(stage, "staged" | "committed"));
    assert!(root.is_absolute() && root.parent() == Some(std::env::temp_dir().as_path()));
    assert!(
        root.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("paisley-crash-record-")
    );
    assert!(!fs::symlink_metadata(root).unwrap().file_type().is_symlink());
    assert_eq!(
        fs::read(root.join("owned-fixture")).unwrap(),
        b"owned profile crash fixture"
    );
    let profile = Profile::open_at(root, "owned", false).unwrap();
    assert_eq!(profile.load_session().unwrap(), snapshot(false));
    if stage == "committed" {
        profile.save_session(&snapshot(true)).unwrap();
        File::create_new(root.join("committed")).unwrap();
        // The parent holds stdin open and kills this process after the marker.
        let _ = std::io::stdin().read_exact(&mut [0u8]);
        panic!("committed child must be killed, not exit normally");
    }
    File::create_new(root.join("ready")).unwrap();
    // Save the same valid replacement repeatedly so any observed record must
    // equal either the complete seed or the complete replacement, never a mix.
    loop {
        profile.save_session(&snapshot(true)).unwrap();
    }
}

#[test]
fn process_interruption_preserves_atomic_profile_records() {
    if let Ok(stage) = std::env::var(CHILD) {
        child(&stage, &PathBuf::from(std::env::var_os(ROOT).unwrap()));
        return;
    }
    for (index, stage) in ["staged", "staged", "staged", "committed"]
        .into_iter()
        .enumerate()
    {
        let root = std::env::temp_dir().join(format!(
            "paisley-crash-record-{}-{index}",
            std::process::id()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        fs::write(root.join("owned-fixture"), b"owned profile crash fixture").unwrap();
        let profile = Profile::open_at(&root, "owned", false).unwrap();
        profile.save_session(&snapshot(false)).unwrap();
        profile.save_settings(&profile.settings).unwrap();
        let settings = fs::read(profile.data_dir.join("settings.json")).unwrap();
        drop(profile);
        let mut writer = Writer(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process_interruption_preserves_atomic_profile_records",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, stage)
                .env(ROOT, &root)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let child_pid = writer.0.id();
        if stage == "staged" {
            wait_for(&mut writer, "owned staged session write", || {
                root.join("ready").is_file()
                    && fs::read_dir(root.join("owned")).unwrap().any(|entry| {
                        let name = entry.unwrap().file_name();
                        name.to_string_lossy()
                            .starts_with(&format!(".browny-{child_pid}-"))
                    })
            });
        } else {
            wait_for(&mut writer, "confirmed committed session", || {
                root.join("committed").is_file()
            });
        }
        writer.0.kill().unwrap();
        let status = writer.0.wait().unwrap();
        assert_eq!(status.signal(), Some(9), "fixture requires actual SIGKILL");
        let reopened = Profile::open_at(&root, "owned", false)
            .expect("kernel must release the killed owner's profile lock");
        let recovered = reopened
            .load_session()
            .expect("session must remain a complete valid record");
        if stage == "committed" {
            assert_eq!(
                recovered,
                snapshot(true),
                "acknowledged commit must survive process interruption"
            );
        } else {
            assert!(
                recovered == snapshot(false) || recovered == snapshot(true),
                "interrupted write produced neither whole valid snapshot"
            );
        }
        assert_eq!(
            fs::read(reopened.data_dir.join("settings.json")).unwrap(),
            settings
        );
        drop(reopened);
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }
}
