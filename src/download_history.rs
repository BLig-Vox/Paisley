//! Bounded terminal download metadata. Records never authorize filesystem changes.
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX_DOWNLOAD_RECORDS: usize = 200;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadRecord {
    pub source: Option<String>,
    pub destination: Option<String>,
    pub outcome: Outcome,
    pub received: u64,
    pub finished: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DownloadHistory {
    pub records: Vec<DownloadRecord>,
}

impl DownloadRecord {
    fn checked(
        source: Option<&str>,
        destination: Option<&Path>,
        outcome: Outcome,
        received: u64,
        finished: u64,
    ) -> Option<Self> {
        let source = source
            .filter(|uri| uri.len() <= 2048 && crate::profile::valid_url(uri))
            .and_then(|uri| url::Url::parse(uri).ok())
            .map(|uri| uri.to_string())
            .filter(|uri| uri.len() <= 2048);
        let destination = destination
            .filter(|path| {
                path.is_absolute()
                    && path.file_name().is_some()
                    && !path
                        .components()
                        .any(|part| matches!(part, std::path::Component::ParentDir))
            })
            .and_then(Path::to_str)
            .filter(|path| path.len() <= 4096 && !path.chars().any(char::is_control))
            .map(str::to_owned);
        if source.is_none() && destination.is_none() {
            return None;
        }
        Some(Self {
            source,
            destination,
            outcome,
            received,
            finished,
        })
    }
    fn budget(&self) -> usize {
        160 + self
            .source
            .iter()
            .chain(&self.destination)
            .map(|s| s.len() + s.bytes().filter(|b| matches!(b, b'"' | b'\\')).count())
            .sum::<usize>()
    }
}

impl DownloadHistory {
    pub fn record(
        &mut self,
        source: Option<&str>,
        destination: Option<&Path>,
        outcome: Outcome,
        received: u64,
        finished: u64,
    ) -> bool {
        let Some(record) =
            DownloadRecord::checked(source, destination, outcome, received, finished)
        else {
            return false;
        };
        self.records.insert(0, record);
        self.trim();
        true
    }
    fn trim(&mut self) {
        self.records.truncate(MAX_DOWNLOAD_RECORDS);
        let mut size = 128
            + self
                .records
                .iter()
                .map(DownloadRecord::budget)
                .sum::<usize>();
        while size > 900_000 {
            let Some(record) = self.records.pop() else {
                break;
            };
            size -= record.budget();
        }
    }
    pub(crate) fn validated(&self) -> Result<Self, String> {
        if self.records.len() > MAX_DOWNLOAD_RECORDS {
            return Err("Too many download records".into());
        }
        let mut result = Self {
            records: self
                .records
                .iter()
                .filter_map(|r| {
                    DownloadRecord::checked(
                        r.source.as_deref(),
                        r.destination.as_deref().map(Path::new),
                        r.outcome,
                        r.received,
                        r.finished,
                    )
                })
                .collect(),
        };
        result.trim();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;

    fn root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "paisley-download-records-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn records_sanitized_metadata_without_unsafe_urls_or_paths() {
        let mut history = DownloadHistory::default();
        assert!(history.record(
            Some("https://user:secret@example.org/file"),
            Some(Path::new("/tmp/fixture.bin")),
            Outcome::Completed,
            4,
            10
        ));
        assert_eq!(history.records[0].source, None);
        assert_eq!(
            history.records[0].destination.as_deref(),
            Some("/tmp/fixture.bin")
        );
        assert_eq!(history.records[0].received, 4);
        assert_eq!(history.records[0].outcome, Outcome::Completed);
        assert!(!history.record(
            Some("file:///etc/passwd"),
            Some(Path::new("relative/file")),
            Outcome::Failed,
            0,
            11
        ));
        assert!(history.record(
            Some("https://example.org/file"),
            Some(Path::new("/tmp/control\nfile")),
            Outcome::Cancelled,
            2,
            12
        ));
        assert_eq!(history.records[0].destination, None);
        assert_eq!(
            history.records[0].source.as_deref(),
            Some("https://example.org/file")
        );
        assert!(!serde_json::to_string(&history).unwrap().contains("secret"));
    }

    #[test]
    fn evicts_old_terminal_records_and_bounds_serialized_size() {
        let mut history = DownloadHistory::default();
        for i in 0..205 {
            history.record(
                Some("https://example.org/file"),
                Some(Path::new(&format!("/tmp/{i}"))),
                Outcome::Completed,
                4,
                i,
            );
        }
        assert_eq!(history.records.len(), 200);
        assert_eq!(history.records[0].finished, 204);
        assert_eq!(history.records.last().unwrap().finished, 5);
        for i in 0..200 {
            history.record(
                Some(&format!("https://example.org/{i}/{}", "x".repeat(1900))),
                Some(Path::new(&format!("/tmp/{i}/{}", "\\".repeat(3800)))),
                Outcome::Failed,
                1,
                i,
            );
        }
        assert!(serde_json::to_vec(&history).unwrap().len() <= 900_000);
        assert!(!history.records.is_empty());
    }

    #[test]
    fn persistence_isolated_private_and_preserves_bad_records() {
        let root = root();
        let normal = Profile::open_at(&root, "work", false).unwrap();
        let mut history = DownloadHistory::default();
        history.record(
            Some("https://example.org/file"),
            Some(Path::new("/tmp/download.bin")),
            Outcome::Completed,
            4,
            10,
        );
        normal.save_download_history(&history).unwrap();
        drop(normal);
        let normal = Profile::open_at(&root, "work", false).unwrap();
        assert_eq!(normal.load_download_history().unwrap(), history);
        let other = Profile::open_at(&root, "other", false).unwrap();
        assert_eq!(
            other.load_download_history().unwrap(),
            DownloadHistory::default()
        );
        let private_root = root.with_extension("private");
        let private = Profile::open_at(&private_root, "private", true).unwrap();
        private.save_download_history(&history).unwrap();
        assert_eq!(
            private.load_download_history().unwrap(),
            DownloadHistory::default()
        );
        assert!(!private_root.exists());
        for bad in ["{", "{\"version\":999,\"records\":[]}"] {
            std::fs::write(normal.data_dir.join("downloads.json"), bad).unwrap();
            assert!(normal.load_download_history().is_err());
            assert!(
                normal
                    .save_download_history(&DownloadHistory::default())
                    .is_err()
            );
            assert_eq!(
                std::fs::read_to_string(normal.data_dir.join("downloads.json")).unwrap(),
                bad
            );
        }
        drop(normal);
        drop(other);
        drop(private);
        std::fs::remove_dir_all(root).unwrap();
    }
}
