//! A single bounded worker serializes profile snapshots off the GTK thread.
use paisley::{
    download_history::DownloadHistory,
    library::Library,
    profile::{Profile, Session},
};
use std::{
    sync::{Arc, mpsc},
    thread,
};

enum Request {
    Save(Session, Library, Option<DownloadHistory>),
    Finish(Session, Library, Option<DownloadHistory>),
}
pub struct SessionWriter {
    send: mpsc::SyncSender<Request>,
    errors: mpsc::Receiver<String>,
    worker: Option<thread::JoinHandle<Result<(), String>>>,
}
impl SessionWriter {
    pub fn new(profile: Arc<Profile>) -> Self {
        let (send, receive) = mpsc::sync_channel(2);
        let (error_send, errors) = mpsc::sync_channel(2);
        let worker = thread::spawn(move || {
            while let Ok(request) = receive.recv() {
                let (session, library, downloads, finish) = match request {
                    Request::Save(s, l, d) => (s, l, d, false),
                    Request::Finish(s, l, d) => (s, l, d, true),
                };
                // Attempt every independent record, even after an earlier error.
                let results = [
                    ("Session", profile.save_session(&session)),
                    ("Library", profile.save_library(&library)),
                    (
                        "Downloads",
                        downloads
                            .as_ref()
                            .map_or(Ok(()), |history| profile.save_download_history(history)),
                    ),
                ];
                let errors: Vec<_> = results
                    .into_iter()
                    .filter_map(|(name, result)| {
                        result.err().map(|error| format!("{name}: {error}"))
                    })
                    .collect();
                let result = if errors.is_empty() {
                    Ok(())
                } else {
                    Err(errors.join("; "))
                };
                if finish {
                    return result;
                }
                if let Err(error) = result {
                    let _ = error_send.try_send(error);
                }
            }
            Ok(())
        });
        Self {
            send,
            errors,
            worker: Some(worker),
        }
    }
    /// False means the latest snapshot should be retried on the next tick.
    pub fn submit(
        &self,
        session: Session,
        library: Library,
        downloads: Option<DownloadHistory>,
    ) -> bool {
        self.send
            .try_send(Request::Save(session, library, downloads))
            .is_ok()
    }
    pub fn error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
    pub fn finish(
        mut self,
        session: Session,
        library: Library,
        downloads: Option<DownloadHistory>,
    ) -> Result<(), String> {
        self.send
            .send(Request::Finish(session, library, downloads))
            .map_err(|e| e.to_string())?;
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| "Session writer stopped unexpectedly".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_download_records_even_when_session_and_library_fail() {
        let root = std::env::temp_dir().join(format!(
            "paisley-download-writer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let profile = Arc::new(Profile::open_at(&root, "work", false).unwrap());
        std::fs::write(profile.data_dir.join("library.json"), "{").unwrap();
        let invalid = Session {
            tabs: vec!["https://example.org/".into(); 201],
            active: 0,
        };
        let mut history = DownloadHistory::default();
        history.record(
            Some("https://example.org/file"),
            Some(std::path::Path::new("/tmp/file.bin")),
            paisley::download_history::Outcome::Completed,
            4,
            10,
        );
        let error = SessionWriter::new(Arc::clone(&profile))
            .finish(invalid.clone(), Library::default(), Some(history.clone()))
            .unwrap_err();
        assert!(error.contains("Session:") && error.contains("Library:"));
        assert_eq!(profile.load_download_history().unwrap(), history);
        assert!(
            SessionWriter::new(Arc::clone(&profile))
                .finish(
                    invalid,
                    Library::default(),
                    Some(DownloadHistory::default())
                )
                .is_err()
        );
        assert!(profile.load_download_history().unwrap().records.is_empty());
        assert_eq!(
            std::fs::read_to_string(profile.data_dir.join("library.json")).unwrap(),
            "{"
        );
        drop(profile);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_session_does_not_prevent_library_save_or_history_deletion() {
        let root = std::env::temp_dir().join(format!(
            "paisley-writer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let profile = Arc::new(Profile::open_at(&root, "work", false).unwrap());
        let mut library = Library::default();
        library.visit("https://example.org/old", "Old history", 1);
        profile.save_library(&library).unwrap();
        library.history.clear();
        library
            .bookmark("https://example.org/new", "New bookmark", 2)
            .unwrap();
        let invalid = Session {
            tabs: vec!["https://example.org/".into(); 201],
            active: 0,
        };
        let writer = SessionWriter::new(Arc::clone(&profile));
        assert!(
            writer
                .finish(invalid, library.clone(), Some(DownloadHistory::default()))
                .is_err()
        );
        assert_eq!(
            profile.load_library().unwrap(),
            library,
            "an unrelated session failure must not resurrect history or lose bookmarks"
        );
        drop(profile);
        std::fs::remove_dir_all(root).unwrap();
    }
}
