//! Durable direct HTTP downloads. Browser-session requests remain in WebKit.
mod transfer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub url: String,
    pub destination: PathBuf,
    pub sha256: Option<String>,
    pub connections: usize,
    pub limit: u64,
    pub completed: Option<String>,
    #[serde(default)]
    pub finished: Option<u64>,
    #[serde(default)]
    pub received: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
}
#[derive(Default, Serialize, Deserialize)]
struct Registry {
    jobs: Vec<Job>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Part {
    start: u64,
    end: Option<u64>,
    committed: u64,
    digest: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    url: String,
    final_url: String,
    total: Option<u64>,
    etag: Option<String>,
    ranged: bool,
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    destination: PathBuf,
    digest: String,
    size: u64,
}

pub struct Control {
    pub stop: AtomicBool,
    pub limit: AtomicU64,
    pub received: AtomicU64,
    pub total: AtomicU64,
    pub status: Mutex<String>,
    pub result: Mutex<Option<Result<String, String>>>,
}
pub struct Worker {
    pub control: Arc<Control>,
    handle: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(job: Job) -> Self {
        let control = Arc::new(Control {
            stop: AtomicBool::new(false),
            limit: AtomicU64::new(job.limit),
            received: AtomicU64::new(0),
            total: AtomicU64::new(0),
            status: Mutex::new("Connecting".into()),
            result: Mutex::new(None),
        });
        let output = control.clone();
        let handle = std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(2)
                .build()
                .map_err(|_| "Could not start transfer runtime".to_string())
                .and_then(|runtime| runtime.block_on(transfer::run(job, output.clone())));
            *output.result.lock().unwrap() = Some(result);
        });
        Self {
            control,
            handle: Some(handle),
        }
    }
    pub fn finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(|handle| handle.is_finished())
    }
    pub fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.control.stop.store(true, Ordering::Relaxed);
        self.join();
    }
}

impl Job {
    pub fn new(
        url: String,
        destination: PathBuf,
        sha256: Option<String>,
        connections: usize,
        limit: u64,
    ) -> Result<Self, String> {
        let id = format!(
            "{:x}-{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos(),
            std::process::id()
        );
        let job = Self {
            id,
            url,
            destination,
            sha256: sha256.map(|s| s.to_ascii_lowercase()),
            connections,
            limit,
            completed: None,
            finished: None,
            received: None,
            total: None,
        };
        job.validate()?;
        Ok(job)
    }
    pub fn validate(&self) -> Result<(), String> {
        validate_url(&self.url)?;
        if self.id.is_empty()
            || self.id.len() > 64
            || !self.id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        {
            return Err("Invalid job identity".into());
        }
        if !(1..=4).contains(&self.connections)
            || self.limit > 512 * 1024 * 1024
            || (self.limit > 0 && self.limit < 16 * 1024)
        {
            return Err("Invalid connection or speed setting".into());
        }
        if self
            .sha256
            .as_ref()
            .is_some_and(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("SHA-256 must contain exactly 64 hexadecimal characters".into());
        }
        if self
            .completed
            .as_ref()
            .is_some_and(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid completion digest".into());
        }
        if self
            .sha256
            .as_ref()
            .zip(self.completed.as_ref())
            .is_some_and(|(expected, actual)| expected != actual)
        {
            return Err("Completion does not match expected checksum".into());
        }
        let path = &self.destination;
        if !path.is_absolute()
            || path.file_name().is_none()
            || path.to_string_lossy().len() > 4096
            || path
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir))
        {
            return Err("Choose an absolute local destination".into());
        }
        Ok(())
    }
    pub fn stage(&self) -> PathBuf {
        self.destination
            .parent()
            .unwrap()
            .join(format!(".paisley-{}.part", self.id))
    }
}
pub fn validate_url(value: &str) -> Result<(), String> {
    let url = url::Url::parse(value).map_err(|_| "Enter a complete HTTP or HTTPS URL")?;
    if value.len() > 8192
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use a credential-free HTTP or HTTPS URL".into());
    }
    Ok(())
}
pub fn load(path: &Path) -> Result<Vec<Job>, String> {
    let mut registry: Registry = crate::profile::read_record(path)?.unwrap_or_default();
    if registry.jobs.len() > 100 {
        return Err("Managed download registry exceeds 100 jobs".into());
    }
    let mut ids = std::collections::HashSet::new();
    for job in &mut registry.jobs {
        job.validate()?;
        if job.completed.is_some() && job.received.is_none() {
            if let Ok(Some(receipt)) =
                crate::profile::read_record::<Receipt>(&job.stage().join("receipt.json"))
            {
                if receipt.destination == job.destination
                    && job.completed.as_ref() == Some(&receipt.digest)
                {
                    job.received = Some(receipt.size);
                }
            }
        }
        if !ids.insert(job.id.clone()) {
            return Err("Duplicate managed job identity".into());
        }
    }
    Ok(registry.jobs)
}
pub fn save(path: &Path, jobs: Vec<Job>) -> Result<(), String> {
    if jobs.len() > 100 {
        return Err("Remove old managed jobs before adding more (limit 100)".into());
    }
    crate::profile::write_record(path, &Registry { jobs })
}
fn open(path: &Path, create: bool) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    if create {
        options.create_new(true);
    }
    let file = options
        .open(path)
        .map_err(|e| format!("Cannot access partial file: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Partial state is not a regular file".into());
    }
    Ok(file)
}
fn hash(file: &mut File, length: u64, control: Option<&Control>) -> Result<Sha256, String> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut remaining = length;
    let mut buffer = [0; 32768];
    while remaining > 0 {
        if control.is_some_and(|c| c.stop.load(Ordering::Relaxed)) {
            return Err("Paused".into());
        }
        let limit = remaining.min(buffer.len() as u64) as usize;
        let n = file.read(&mut buffer[..limit]).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Partial file is truncated".into());
        }
        digest.update(&buffer[..n]);
        remaining -= n as u64;
    }
    Ok(digest)
}
fn hex(digest: &Sha256) -> String {
    format!("{:x}", digest.clone().finalize())
}
