//! Explicit bounded bookmark interchange, separate from history/session records.
use crate::library::{Library, MAX_BOOKMARKS, Page};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

pub const MAX_IMPORT_BYTES: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transfer {
    version: u32,
    bookmarks: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    uri: String,
    title: String,
}

#[derive(Clone)]
pub struct BookmarkImport {
    pages: Vec<Page>,
}
impl BookmarkImport {
    pub fn count(&self) -> usize {
        self.pages.len()
    }
}

pub fn parse_import(bytes: &[u8]) -> Result<BookmarkImport, String> {
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err("Bookmark import exceeds 1 MiB".into());
    }
    let transfer: Transfer = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "Unsupported bookmark schema at line {}, column {}",
            error.line(),
            error.column()
        )
    })?;
    if transfer.version != 1 {
        return Err("Unsupported bookmark transfer version".into());
    }
    if transfer.bookmarks.len() > MAX_BOOKMARKS {
        return Err("Bookmark import exceeds 256 entries".into());
    }
    let mut seen = HashSet::new();
    let mut pages = Vec::new();
    for entry in transfer.bookmarks {
        if entry.uri.len() > 2048
            || !crate::profile::valid_url(&entry.uri)
            || entry.title.chars().count() > 256
            || entry.title.chars().any(char::is_control)
        {
            return Err("Bookmarks require credential-free HTTP(S) URLs up to 2048 bytes and plain-text titles up to 256 characters without controls".into());
        }
        let uri = url::Url::parse(&entry.uri)
            .map_err(|_| "Invalid bookmark URL")?
            .to_string();
        if uri.len() > 2048 || !seen.insert(uri.clone()) {
            return Err("Bookmark import contains an oversized or duplicate canonical URL".into());
        }
        pages.push(Page {
            uri,
            title: entry.title,
            visited: 0,
        });
    }
    Ok(BookmarkImport { pages })
}

pub fn merge_import(current: &Library, import: &BookmarkImport) -> Result<Library, String> {
    let mut next = current.clone();
    let mut seen: HashSet<_> = next.bookmarks.iter().map(|page| page.uri.clone()).collect();
    next.bookmarks.extend(
        import
            .pages
            .iter()
            .filter(|page| seen.insert(page.uri.clone()))
            .cloned(),
    );
    let next = next.validated()?;
    if next.history != current.history {
        return Err("Import would exceed the library budget and evict history; remove records explicitly first".into());
    }
    Ok(next)
}

pub fn export_bookmarks(library: &Library) -> Result<Vec<u8>, String> {
    let transfer = Transfer {
        version: 1,
        bookmarks: library
            .bookmarks
            .iter()
            .map(|page| Entry {
                uri: page.uri.clone(),
                title: page.title.clone(),
            })
            .collect(),
    };
    let mut bytes =
        serde_json::to_vec_pretty(&transfer).map_err(|_| "Cannot serialize bookmarks")?;
    // Apply the same safety/size rules to exports, not just imported data.
    parse_import(&bytes)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err("Bookmark export exceeds 1 MiB".into());
    }
    Ok(bytes)
}
pub fn load_file(path: &Path) -> Result<BookmarkImport, String> {
    parse_import(&crate::transfer_io::read_bounded(path, MAX_IMPORT_BYTES)?)
}
pub fn save_export(path: &Path, library: &Library) -> Result<(), String> {
    crate::transfer_io::write_new(path, &export_bookmarks(library)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::MAX_BOOKMARKS;

    fn input(entries: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"version": 1, "bookmarks": entries})).unwrap()
    }

    #[test]
    fn export_roundtrips_only_bookmarks_without_history_or_timestamps() {
        let mut current = Library::default();
        current.visit("https://history.test/private", "history secret", 99);
        current
            .bookmark("https://example.test/a", "<b>Crab 🦀</b>", 123)
            .unwrap();
        let bytes = export_bookmarks(&current).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"version": 1, "bookmarks": [{"uri": "https://example.test/a", "title": "<b>Crab 🦀</b>"}]})
        );
        let imported = parse_import(&bytes).unwrap();
        assert_eq!(imported.count(), 1);
        let merged = merge_import(&Library::default(), &imported).unwrap();
        assert!(merged.history.is_empty());
        assert_eq!(merged.bookmarks[0].title, "<b>Crab 🦀</b>");
        assert_eq!(merged.bookmarks[0].visited, 0);
    }

    #[test]
    fn additive_merge_preserves_current_conflicts_history_and_source_order() {
        let mut current = Library::default();
        current
            .bookmark("https://example.test/existing", "Keep this title", 37)
            .unwrap();
        current.visit("https://history.test/", "Keep history", 42);
        let before = current.clone();
        let import = parse_import(&input(serde_json::json!([
            {"uri": "https://EXAMPLE.test:443/existing", "title": "Do not replace"},
            {"uri": "https://example.test/first", "title": "First"},
            {"uri": "https://example.test/second", "title": "Second"}
        ])))
        .unwrap();
        let next = merge_import(&current, &import).unwrap();
        assert_eq!(next.history, before.history);
        assert_eq!(next.bookmarks[0], before.bookmarks[0]);
        assert_eq!(
            next.bookmarks
                .iter()
                .map(|p| p.uri.as_str())
                .collect::<Vec<_>>(),
            [
                "https://example.test/existing",
                "https://example.test/first",
                "https://example.test/second"
            ]
        );
        assert_eq!(current, before);
        assert_eq!(
            merge_import(&next, &import).unwrap(),
            next,
            "reimport is idempotent"
        );
    }

    #[test]
    fn merge_refuses_capacity_without_evicting_existing_records() {
        let mut current = Library::default();
        for n in 0..MAX_BOOKMARKS {
            current
                .bookmark(&format!("https://example.test/{n}"), "Saved", 1)
                .unwrap();
        }
        let before = current.clone();
        let import = parse_import(&input(
            serde_json::json!([{ "uri": "https://new.test/", "title": "New" }]),
        ))
        .unwrap();
        assert!(merge_import(&current, &import).is_err());
        assert_eq!(current, before);
        let mut current = Library::default();
        for n in 0..1000 {
            current.visit(
                &format!("https://history.test/{n}/{}", "a".repeat(650)),
                "H",
                n,
            );
        }
        let before = current.clone();
        let entries: Vec<_> = (0..256).map(|n| serde_json::json!({"uri": format!("https://new.test/{n}/{}", "a".repeat(1500)), "title": "B"})).collect();
        let import = parse_import(&input(serde_json::json!(entries))).unwrap();
        assert!(
            merge_import(&current, &import).is_err(),
            "import must not cause ordinary library size retention to evict history"
        );
        assert_eq!(current, before);
    }

    #[test]
    fn schema_rejects_unsafe_duplicate_unbounded_or_unsupported_data() {
        for bytes in [
            b"{}".to_vec(),
            br#"{"version":2,"bookmarks":[]}"#.to_vec(),
            br#"{"version":1,"version":1,"bookmarks":[]}"#.to_vec(),
            br#"{"version":1,"bookmarks":[],"secret-payload":true}"#.to_vec(),
            br#"{"version":1,"bookmarks":null}"#.to_vec(),
            input(serde_json::json!([{"uri": "file:///etc/passwd", "title": "Bad"}])),
            input(serde_json::json!([{"uri": "https://user:secret@example.test/", "title": "Bad"}])),
            input(serde_json::json!([{"uri": "javascript:alert(1)", "title": "Bad"}])),
            input(serde_json::json!([{"uri": "https://example.test/", "title": "x\nsecret-payload"}])),
            input(serde_json::json!([{"uri": "https://example.test/", "title": "🦀".repeat(257)}])),
            input(serde_json::json!([{"uri": format!("https://example.test/{}", "a".repeat(2048)), "title": "Bad"}])),
            input(serde_json::json!([{"uri": "https://example.test/", "title": "A"}, {"uri": "https://EXAMPLE.test:443/", "title": "B"}])),
            input(serde_json::json!([{"uri": "https://example.test/", "title": "A", "visited": 99}])),
            input(serde_json::json!([{"uri": "https://example.test/", "title": false}])),
            input(serde_json::json!((0..257).map(|n| serde_json::json!({"uri": format!("https://example.test/{n}"), "title": "A"})).collect::<Vec<_>>())),
            vec![b' '; 1024 * 1024 + 1],
        ] {
            let error = parse_import(&bytes).err().expect("invalid import accepted");
            assert!(!error.contains("secret-payload"), "errors must not echo file contents");
        }
        let import = parse_import(&input(
            serde_json::json!([{ "uri": "https://EXAMPLE.test:443", "title": "🦀".repeat(256) }]),
        ))
        .unwrap();
        assert_eq!(
            merge_import(&Library::default(), &import)
                .unwrap()
                .bookmarks[0]
                .uri,
            "https://example.test/"
        );
        assert_eq!(
            parse_import(&input(serde_json::json!([]))).unwrap().count(),
            0
        );
    }

    #[test]
    fn export_refuses_unsafe_in_memory_records_before_creating_a_file() {
        let directory =
            std::env::temp_dir().join(format!("paisley-invalid-export-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        for (uri, title) in [
            ("https://user:secret@example.test/", "A".to_owned()),
            ("https://example.test/", "x".repeat(257)),
        ] {
            let library = Library {
                bookmarks: vec![Page {
                    uri: uri.into(),
                    title,
                    visited: 1,
                }],
                ..Library::default()
            };
            assert!(save_export(&directory.join("out.json"), &library).is_err());
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        }
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn file_transfer_is_private_regular_bounded_and_never_overwrites() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = std::env::temp_dir().join(format!(
            "paisley-bookmark-transfer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("bookmarks.json");
        let mut library = Library::default();
        library
            .bookmark("https://example.test/", "Saved", 7)
            .unwrap();
        save_export(&path, &library).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(load_file(&path).unwrap().count(), 1);
        assert!(save_export(&path, &Library::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let link = directory.join("link");
        symlink(&path, &link).unwrap();
        assert!(load_file(&link).is_err());
        assert!(save_export(&link, &library).is_err());
        assert!(load_file(&directory).is_err());
        let oversized = directory.join("large");
        std::fs::write(&oversized, vec![b' '; 1024 * 1024 + 1]).unwrap();
        assert!(load_file(&oversized).is_err());
        assert_eq!(
            std::fs::read_dir(&directory).unwrap().count(),
            3,
            "staging must be removed after success and refusal"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
