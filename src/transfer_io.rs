//! Shared local-file boundaries for explicit settings/bookmark transfers.
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    options.custom_flags(0x20000 | 0x800); // O_NOFOLLOW | O_NONBLOCK, before fstat.
    let mut file = options
        .open(path)
        .map_err(|error| format!("Cannot open import file: {error}"))?;
    let info = file.metadata().map_err(|error| error.to_string())?;
    if !info.is_file() || info.len() > limit as u64 {
        return Err(format!(
            "Import requires a regular file of at most {limit} bytes"
        ));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit || bytes.len() as u64 != info.len() {
        return Err("Import file changed while reading".into());
    }
    Ok(bytes)
}

pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().ok_or("Missing export directory")?;
    let temporary = parent.join(format!(
        ".browny-transfer-{}-{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    let result = (|| {
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::hard_link(&temporary, path)
            .map_err(|error| format!("Export not saved; choose a new filename: {error}"))
    })();
    let cleanup = fs::remove_file(&temporary)
        .map_err(|error| format!("Could not remove export staging file: {error}"));
    result.and(cleanup)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn fifo_import_rejects_promptly_without_waiting_for_a_writer() {
        unsafe extern "C" {
            fn mkfifo(path: *const std::ffi::c_char, mode: u32) -> i32;
        }
        let directory =
            std::env::temp_dir().join(format!("browny-transfer-fifo-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("pipe");
        let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: valid NUL-terminated owned path and Linux mode_t bits; no
        // foreign pointer is retained. This only creates our disposable FIFO.
        assert_eq!(unsafe { mkfifo(name.as_ptr(), 0o600) }, 0);
        let (sender, receiver) = std::sync::mpsc::channel();
        let input = path.clone();
        let worker = std::thread::spawn(move || {
            sender.send(read_bounded(&input, 64 * 1024)).unwrap();
        });
        let result = receiver.recv_timeout(std::time::Duration::from_secs(1));
        if result.is_err() {
            // Release an accidentally blocking reader so a regression does not
            // strand the worker or leave the whole test suite hung.
            if let Ok(mut writer) = OpenOptions::new()
                .write(true)
                .custom_flags(0x800)
                .open(&path)
            {
                let _ = writer.write_all(b"x");
            }
        }
        worker.join().unwrap();
        fs::remove_dir_all(directory).unwrap();
        assert!(
            matches!(result, Ok(Err(_))),
            "FIFO was not rejected promptly"
        );
    }
}
