//! Profile-local settings and bounded browser session persistence.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_TABS: usize = 200;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub restore_session: bool,
    pub remember_history: bool,
    pub homepage: String,
    pub show_memory: bool,
    pub block_trackers: bool,
    pub strict_privacy: bool,
    pub download_directory: Option<PathBuf>,
    pub agent_enabled: bool,
    pub blocking_exceptions: Vec<String>,
    pub notification_origins: Vec<String>,
    pub desktop_notifications: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            restore_session: true,
            remember_history: true,
            homepage: "https://duckduckgo.com/".into(),
            show_memory: true,
            block_trackers: true,
            strict_privacy: false,
            download_directory: None,
            agent_enabled: false,
            blocking_exceptions: Vec::new(),
            notification_origins: Vec::new(),
            desktop_notifications: true,
        }
    }
}
/// Metadata-only sentinel: never pass this to WebKit or treat it as a URL grant.
// Kept stable across the Paisley rename for saved-session compatibility.
pub const NON_RESTORABLE_TAB: &str = "browny:non-restorable";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<String>,
    pub active: usize,
}

#[derive(Serialize, Deserialize)]
struct Record<T> {
    version: u32,
    #[serde(flatten)]
    value: T,
}

pub struct Profile {
    pub id: String,
    pub private: bool,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub settings: Settings,
    settings_dir: PathBuf,
    _lock: Option<File>,
}

pub(crate) fn valid_url(s: &str) -> bool {
    // Requiring the explicit scheme avoids resolve's search and host heuristics.
    let Some((scheme, rest)) = s.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    if rest.is_empty() || s.trim() != s || s.chars().any(char::is_control) {
        return false;
    }
    matches!(
        crate::navigation::resolve(s),
        Ok(crate::navigation::Navigation::Url(_))
    )
}
fn validate_settings(s: &Settings) -> Result<(), String> {
    if !valid_url(&s.homepage) {
        return Err("Homepage must be a credential-free HTTP or HTTPS URL".into());
    }
    if s.notification_origins.len() > 128 {
        return Err("Too many notification sites (maximum 128)".into());
    }
    for origin in &s.notification_origins {
        let parsed = url::Url::parse(origin).map_err(|_| "Invalid notification origin")?;
        let local = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !(parsed.scheme() == "https" || (parsed.scheme() == "http" && local))
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.origin().ascii_serialization() != *origin
        {
            return Err(
                "Notification sites must be canonical HTTPS origins (or local HTTP)".into(),
            );
        }
    }
    Ok(())
}
/// Normalize one session metadata entry without retaining transient payloads.
/// The returned sentinel is not a navigable URL or a permission grant.
pub fn session_target(uri: &str) -> Option<&str> {
    let transient = uri.split_once(':').is_some_and(|(scheme, rest)| {
        scheme.eq_ignore_ascii_case("data")
            || scheme.eq_ignore_ascii_case("blob")
            || (scheme.eq_ignore_ascii_case("about")
                && rest.split(['#', '?']).next().is_some_and(|page| {
                    page.eq_ignore_ascii_case("blank") || page.eq_ignore_ascii_case("srcdoc")
                }))
    });
    if valid_url(uri) {
        Some(uri)
    } else if uri == NON_RESTORABLE_TAB || transient {
        Some(NON_RESTORABLE_TAB)
    } else {
        None
    }
}

fn checked_session(s: &Session) -> Result<Session, String> {
    if s.tabs.len() > MAX_TABS {
        return Err("Too many session tabs".into());
    }
    let mut tabs = Vec::new();
    let mut active = 0;
    for (i, uri) in s.tabs.iter().enumerate() {
        if let Some(saved) = session_target(uri) {
            if i <= s.active {
                active = tabs.len();
            }
            tabs.push(saved.to_owned());
        }
    }
    Ok(Session { tabs, active })
}
pub(crate) fn reject_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            Err(format!("Symlink not allowed: {}", path.display()))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
pub(crate) fn ensure_dir(path: &Path) -> Result<(), String> {
    // Reject symlinks in every existing component, including parents.
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        reject_symlink(&prefix)?;
        if !prefix.exists() {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(&prefix) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => reject_symlink(&prefix)?,
                Err(e) => return Err(format!("{}: {e}", prefix.display())),
            }
        }
        if !prefix.is_dir() {
            return Err(format!("Not a directory: {}", prefix.display()));
        }
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    Ok(())
}
pub(crate) fn open_options() -> OpenOptions {
    let mut o = OpenOptions::new();
    #[cfg(unix)]
    o.mode(0o600);
    #[cfg(target_os = "linux")]
    o.custom_flags(0x20000 | 0x800); // Linux O_NOFOLLOW | O_NONBLOCK; regular files are unaffected.
    o
}
pub(crate) fn maintenance_lock(root: &Path) -> Result<File, String> {
    ensure_dir(root)?;
    let file = open_options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".browny-maintenance.lock"))
        .map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Maintenance lock is not a regular file".into());
    }
    file.try_lock().map_err(|error| {
        format!("Profile maintenance is busy; retry after it finishes: {error}")
    })?;
    Ok(file)
}

pub(crate) fn read_record<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    reject_symlink(path)?;
    let file = match open_options().read(true).open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("State is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("State file exceeds size limit".into());
    }
    let record: Record<T> = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid state {}: {e}", path.display()))?;
    if record.version != 1 {
        return Err(format!("Unsupported state version {}", record.version));
    }
    Ok(Some(record.value))
}
pub(crate) fn write_record<T: Serialize + DeserializeOwned>(
    path: &Path,
    value: &T,
) -> Result<(), String> {
    // Refuse to replace records we could not interpret, including newer versions.
    let _: Option<T> = read_record(path)?;
    let bytes = serde_json::to_vec(&Record { version: 1, value }).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("State exceeds size limit".into());
    }
    let parent = path.parent().ok_or("Missing state directory")?;
    let temp = parent.join(format!(
        ".browny-{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = open_options()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        reject_symlink(path)?;
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
pub(crate) fn xdg(var: &str, fallback: &str) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("HOME is not an absolute path")?;
    Ok(home.join(fallback))
}
impl Profile {
    pub fn open(id: &str, private: bool) -> Result<Self, String> {
        Self::validate_id(id)?;
        if private {
            return Self::open_at(Path::new(""), id, true);
        }
        Self::open_dirs(
            id,
            xdg("XDG_CONFIG_HOME", ".config")?
                .join("browny/profiles")
                .join(id),
            xdg("XDG_DATA_HOME", ".local/share")?
                .join("browny/profiles")
                .join(id),
            xdg("XDG_CACHE_HOME", ".cache")?
                .join("browny/profiles")
                .join(id),
            false,
        )
    }
    pub fn open_at(base: &Path, id: &str, private: bool) -> Result<Self, String> {
        Self::validate_id(id)?;
        let data = base.join(id);
        Self::open_dirs(id, data.clone(), data.clone(), data.join("cache"), private)
    }
    pub fn validate_id(id: &str) -> Result<(), String> {
        if id.is_empty()
            || id.len() > 64
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("Invalid profile ID".into());
        }
        Ok(())
    }
    fn open_dirs(
        id: &str,
        settings_dir: PathBuf,
        data_dir: PathBuf,
        cache_dir: PathBuf,
        private: bool,
    ) -> Result<Self, String> {
        if private {
            return Ok(Self {
                id: id.into(),
                private,
                data_dir,
                cache_dir,
                settings: Settings::default(),
                settings_dir,
                _lock: None,
            });
        }
        // A stable lock outside the deletable profile directory coordinates
        // startup with catalog maintenance even while profile.lock is unlinked.
        let _maintenance = maintenance_lock(data_dir.parent().ok_or("Missing profile data root")?)?;
        ensure_dir(&data_dir)?;
        ensure_dir(&settings_dir)?;
        ensure_dir(&cache_dir)?;
        let lock_path = data_dir.join("profile.lock");
        reject_symlink(&lock_path)?;
        let lock = open_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| e.to_string())?;
        if !lock.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Lock is not a regular file".into());
        }
        #[cfg(unix)]
        lock.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|e| format!("Profile is locked: {e}"))?;
        let settings: Settings =
            read_record(&settings_dir.join("settings.json"))?.unwrap_or_default();
        validate_settings(&settings)?;
        Ok(Self {
            id: id.into(),
            private,
            data_dir,
            cache_dir,
            settings,
            settings_dir,
            _lock: Some(lock),
        })
    }
    pub fn load_download_history(
        &self,
    ) -> Result<crate::download_history::DownloadHistory, String> {
        if self.private {
            return Ok(crate::download_history::DownloadHistory::default());
        }
        read_record::<crate::download_history::DownloadHistory>(
            &self.data_dir.join("downloads.json"),
        )?
        .unwrap_or_default()
        .validated()
    }
    pub fn save_download_history(
        &self,
        history: &crate::download_history::DownloadHistory,
    ) -> Result<(), String> {
        if self.private {
            return Ok(());
        }
        write_record(&self.data_dir.join("downloads.json"), &history.validated()?)
    }
    pub fn load_library(&self) -> Result<crate::library::Library, String> {
        if self.private {
            return Ok(crate::library::Library::default());
        }
        read_record::<crate::library::Library>(&self.data_dir.join("library.json"))?
            .unwrap_or_default()
            .validated()
    }
    pub fn save_library(&self, library: &crate::library::Library) -> Result<(), String> {
        if self.private {
            return Ok(());
        }
        write_record(&self.data_dir.join("library.json"), &library.validated()?)
    }
    pub fn load_session(&self) -> Result<Session, String> {
        if self.private {
            return Ok(Session::default());
        }
        checked_session(
            &read_record::<Session>(&self.data_dir.join("session.json"))?.unwrap_or_default(),
        )
    }
    pub fn save_session(&self, session: &Session) -> Result<(), String> {
        if self.private {
            return Ok(());
        }
        write_record(
            &self.data_dir.join("session.json"),
            &checked_session(session)?,
        )
    }
    pub fn display_name(&self) -> Result<String, String> {
        if self.private {
            return Ok(self.id.clone());
        }
        let root = self
            .settings_dir
            .parent()
            .ok_or("Missing profile settings root")?;
        crate::profile_catalog::ProfileCatalog::at(root, false).display_name(&self.id)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        if self.private {
            return Ok(());
        }
        validate_settings(settings)?;
        write_record(&self.settings_dir.join("settings.json"), settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn base() -> PathBuf {
        std::env::temp_dir().join(format!(
            "paisley-profile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    #[test]
    fn catalog_renames_only_closed_profile_without_changing_data() {
        let b = base();
        let p = Profile::open_at(&b, "work", false).unwrap();
        let catalog = crate::profile_catalog::ProfileCatalog::at(&b, false);
        assert!(catalog.rename("work", "Work profile").is_err());
        drop(p);
        catalog.rename("work", "Work profile").unwrap();
        let entries = catalog.list().unwrap();
        assert_eq!(entries[0].id, "work");
        assert_eq!(entries[0].name, "Work profile");
        assert!(b.join("work/profile.lock").exists());
        let reopened = Profile::open_at(&b, "work", false).unwrap();
        assert_eq!(reopened.display_name().unwrap(), "Work profile");
        assert_eq!(reopened.id, "work");
        drop(reopened);
        std::fs::remove_dir_all(b).unwrap();
    }
    #[test]
    fn profile_startup_and_catalog_mutations_respect_maintenance_lock() {
        let b = base();
        drop(Profile::open_at(&b, "work", false).unwrap());
        let maintenance = open_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(b.join(".browny-maintenance.lock"))
            .unwrap();
        maintenance.try_lock().unwrap();
        let opening = Profile::open_at(&b, "work", false);
        let open_refused = opening.is_err();
        drop(opening);
        let catalog = crate::profile_catalog::ProfileCatalog::at(&b, false);
        let rename_refused = catalog.rename("work", "Changed during deletion").is_err();
        let delete_refused = catalog.delete("work", true).is_err();
        drop(maintenance);
        std::fs::remove_dir_all(b).unwrap();
        assert!(open_refused, "startup ignored the maintenance lock");
        assert!(
            rename_refused && delete_refused,
            "catalog mutation ignored the maintenance lock"
        );
    }

    #[test]
    fn catalog_requires_confirmation_and_exclusive_lock() {
        let b = base();
        let p = Profile::open_at(&b, "work", false).unwrap();
        let c = crate::profile_catalog::ProfileCatalog::at(&b, false);
        assert!(c.delete("work", false).is_err());
        assert!(c.delete("work", true).is_err());
        assert!(b.join("work").exists());
        drop(p);
        assert!(c.delete("work", false).is_err());
        assert!(b.join("work").exists());
        c.delete("work", true).unwrap();
        assert!(!b.join("work").exists());
        std::fs::remove_dir_all(b).unwrap();
    }
    #[test]
    fn catalog_preserves_bad_metadata_and_private_writes_nothing() {
        let b = base();
        let p = Profile::open_at(&b, "work", false).unwrap();
        drop(p);
        let c = crate::profile_catalog::ProfileCatalog::at(&b, false);
        let path = b.join("work/profile.json");
        for text in ["{", "{\"version\":999,\"name\":\"future\"}"] {
            std::fs::write(&path, text).unwrap();
            assert!(c.list().is_err());
            assert!(c.rename("work", "new").is_err());
            assert!(c.delete("work", true).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        }
        let private = crate::profile_catalog::ProfileCatalog::at(&b, true);
        assert!(private.list().unwrap().is_empty());
        assert!(private.rename("work", "new").is_err());
        assert!(private.delete("work", true).is_err());
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "{\"version\":999,\"name\":\"future\"}"
        );
        std::fs::remove_dir_all(b).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn catalog_rejects_invalid_and_symlink_paths() {
        use std::os::unix::fs::symlink;
        let b = base();
        std::fs::create_dir_all(&b).unwrap();
        symlink(&b, b.join("alias")).unwrap();
        let c = crate::profile_catalog::ProfileCatalog::at(&b, false);
        assert!(c.list().is_err());
        assert!(c.rename("../escape", "name").is_err());
        assert!(c.delete("alias", true).is_err());
        let alias = crate::profile_catalog::ProfileCatalog::at(&b.join("alias"), false);
        assert!(alias.list().is_err());
        std::fs::remove_file(b.join("alias")).unwrap();
        std::fs::remove_dir_all(b).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn catalog_rejects_fifo_metadata_without_waiting_for_a_writer() {
        use std::os::unix::ffi::OsStrExt;
        unsafe extern "C" {
            fn mkfifo(path: *const std::ffi::c_char, mode: u32) -> i32;
        }
        let b = base();
        drop(Profile::open_at(&b, "work", false).unwrap());
        let path = b.join("work/profile.json");
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // Test-only creation of an owned FIFO; no external program or input.
        assert_eq!(unsafe { mkfifo(c_path.as_ptr(), 0o600) }, 0);
        let catalog = crate::profile_catalog::ProfileCatalog::at(&b, false);
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = tx.send(catalog.list());
        });
        let result = rx.recv_timeout(std::time::Duration::from_secs(1));
        if matches!(result, Err(std::sync::mpsc::RecvTimeoutError::Timeout)) {
            // Release the old blocking reader before asserting the regression.
            let mut writer = OpenOptions::new().write(true).open(&path).unwrap();
            writer
                .write_all(br#"{"version":1,"name":"Owned"}"#)
                .unwrap();
        }
        worker.join().unwrap();
        std::fs::remove_dir_all(b).unwrap();
        assert!(
            matches!(result, Ok(Err(_))),
            "nonregular metadata waited for a writer instead of failing closed"
        );
    }

    #[test]
    fn rejects_invalid_ids() {
        let b = base();
        for id in ["", "../x", ".", "a/b", "a b", "é", &"x".repeat(65)] {
            assert!(Profile::open_at(&b, id, false).is_err(), "{id}");
        }
        assert!(!b.exists());
    }
    #[test]
    fn locks_and_releases_independent_profiles() {
        let b = base();
        let a = Profile::open_at(&b, "a", false).unwrap();
        assert!(Profile::open_at(&b, "a", false).is_err());
        let other = Profile::open_at(&b, "b", false).unwrap();
        drop(a);
        assert!(Profile::open_at(&b, "a", false).is_ok());
        drop(other);
        std::fs::remove_dir_all(b).unwrap();
    }
    #[test]
    fn persists_settings_and_filters_session() {
        let b = base();
        let p = Profile::open_at(&b, "a", false).unwrap();
        let s = Settings {
            show_memory: false,
            ..Settings::default()
        };
        p.save_settings(&s).unwrap();
        p.save_session(&Session {
            tabs: vec![
                "https://example.com/".into(),
                "file:///tmp/a".into(),
                "https://u:p@example.com".into(),
            ],
            active: 2,
        })
        .unwrap();
        drop(p);
        let p = Profile::open_at(&b, "a", false).unwrap();
        assert!(!p.settings.show_memory);
        let s = p.load_session().unwrap();
        assert_eq!(s.tabs, vec!["https://example.com/"]);
        assert_eq!(s.active, 0);
        drop(p);
        std::fs::remove_dir_all(b).unwrap();
    }
    #[test]
    fn transient_session_documents_become_payload_free_placeholders() {
        let b = base();
        let p = Profile::open_at(&b, "owned", false).unwrap();
        let marker = "browny:non-restorable";
        p.save_session(&Session {
            tabs: vec![
                "https://example.test/first".into(),
                format!("data:text/html,SECRET_PAYLOAD_{}", "x".repeat(1_100_000)),
                "blob:https://example.test/SECRET_BLOB".into(),
                "about:blank#SECRET_FRAGMENT".into(),
                "about:srcdoc".into(),
                marker.into(),
                "https://example.test/last".into(),
                "file:///SECRET_LOCAL".into(),
                "javascript:SECRET_SCRIPT".into(),
                "https://SECRET_USER:SECRET_PASSWORD@example.test/".into(),
                "browny:non-restorable#SECRET_SUFFIX".into(),
            ],
            active: 3,
        })
        .unwrap();
        let expected = Session {
            tabs: vec![
                "https://example.test/first".into(),
                marker.into(),
                marker.into(),
                marker.into(),
                marker.into(),
                marker.into(),
                "https://example.test/last".into(),
            ],
            active: 3,
        };
        assert_eq!(p.load_session().unwrap(), expected);
        let record = fs::read(p.data_dir.join("session.json")).unwrap();
        assert!(
            record.len() < 1024,
            "transient payload must not enter the record"
        );
        assert!(!String::from_utf8(record).unwrap().contains("SECRET_"));
        drop(p);
        let reopened = Profile::open_at(&b, "owned", false).unwrap();
        assert_eq!(reopened.load_session().unwrap(), expected);
        drop(reopened);
        fs::remove_dir_all(b).unwrap();
    }

    #[test]
    fn private_never_writes() {
        let b = base();
        let p = Profile::open_at(&b, "a", true).unwrap();
        p.save_settings(&Settings::default()).unwrap();
        p.save_session(&Session {
            tabs: vec![
                "data:text/html,PRIVATE_PAYLOAD".into(),
                NON_RESTORABLE_TAB.into(),
            ],
            active: 1,
        })
        .unwrap();
        assert!(p.load_session().unwrap().tabs.is_empty());
        assert!(!b.exists());
    }
    #[test]
    fn corruption_and_versions_do_not_overwrite() {
        let b = base();
        let p = Profile::open_at(&b, "a", false).unwrap();
        let path = p.data_dir.join("session.json");
        for contents in ["{", "{\"version\":999,\"tabs\":[]}"] {
            std::fs::write(&path, contents).unwrap();
            assert!(p.load_session().is_err());
            assert!(p.save_session(&Session::default()).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
        }
        drop(p);
        std::fs::remove_dir_all(b).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlinks() {
        use std::os::unix::fs::symlink;
        let b = base();
        std::fs::create_dir_all(&b).unwrap();
        symlink(&b, b.join("a")).unwrap();
        assert!(Profile::open_at(&b, "a", false).is_err());
        std::fs::remove_file(b.join("a")).unwrap();
        let p = Profile::open_at(&b, "a", false).unwrap();
        symlink(b.join("victim"), p.data_dir.join("session.json")).unwrap();
        assert!(p.load_session().is_err());
        assert!(p.save_session(&Session::default()).is_err());
        drop(p);
        std::fs::remove_dir_all(b).unwrap();
    }
}
