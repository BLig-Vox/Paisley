//! Bounded per-profile bookmarks and history. Page strings are untrusted data.
use serde::{Deserialize, Serialize};

pub const MAX_HISTORY: usize = 1000;
pub const MAX_BOOKMARKS: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Page {
    pub uri: String,
    pub title: String,
    pub visited: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Library {
    pub history: Vec<Page>,
    pub bookmarks: Vec<Page>,
}

impl Page {
    fn checked(uri: &str, title: &str, visited: u64) -> Option<Self> {
        if uri.len() > 2048 || !crate::profile::valid_url(uri) {
            return None;
        }
        let uri = url::Url::parse(uri).ok()?.to_string();
        if uri.len() > 2048 {
            return None;
        }
        Some(Self {
            uri,
            title: title
                .chars()
                .filter(|c| !c.is_control())
                .take(256)
                .collect(),
            visited,
        })
    }
    fn budget(&self) -> usize {
        // Conservative JSON size without serializing the entire library on GTK.
        self.uri.len()
            + self.title.len()
            + self
                .title
                .bytes()
                .filter(|b| matches!(b, b'"' | b'\\'))
                .count()
            + 128
    }
}

impl Library {
    pub fn visit(&mut self, uri: &str, title: &str, visited: u64) -> bool {
        let Some(page) = Page::checked(uri, title, visited) else {
            return false;
        };
        if self.history.first() == Some(&page) {
            return false;
        }
        self.history.retain(|p| p.uri != page.uri);
        self.history.insert(0, page);
        self.history.truncate(MAX_HISTORY);
        self.trim_history();
        true
    }
    pub fn update_title(&mut self, uri: &str, title: &str) -> bool {
        let Some(page) = Page::checked(uri, title, 0) else {
            return false;
        };
        let Some(existing) = self.history.iter_mut().find(|p| p.uri == page.uri) else {
            return false;
        };
        if existing.title == page.title {
            return false;
        }
        existing.title = page.title;
        self.trim_history();
        true
    }
    pub fn bookmark(&mut self, uri: &str, title: &str, visited: u64) -> Result<(), String> {
        let page = Page::checked(uri, title, visited)
            .ok_or("Bookmarks require a credential-free HTTP(S) URL of at most 2048 bytes")?;
        if self.bookmarks.len() >= MAX_BOOKMARKS
            && !self.bookmarks.iter().any(|p| p.uri == page.uri)
        {
            return Err(format!(
                "Bookmark limit reached ({MAX_BOOKMARKS}); remove a bookmark first"
            ));
        }
        self.bookmarks.retain(|p| p.uri != page.uri);
        self.bookmarks.insert(0, page);
        self.trim_history();
        Ok(())
    }
    pub fn remove_bookmark(&mut self, uri: &str) {
        self.bookmarks.retain(|p| p.uri != uri);
    }
    pub fn search(&self, bookmarks: bool, query: &str) -> Vec<Page> {
        let query: String = query
            .trim()
            .chars()
            .take(256)
            .collect::<String>()
            .to_lowercase();
        let pages = if bookmarks {
            &self.bookmarks
        } else {
            &self.history
        };
        pages
            .iter()
            .filter(|p| {
                p.uri.to_lowercase().contains(&query) || p.title.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }
    fn trim_history(&mut self) {
        let mut bytes = 128
            + self
                .history
                .iter()
                .chain(&self.bookmarks)
                .map(Page::budget)
                .sum::<usize>();
        while bytes > 900_000 {
            let Some(page) = self.history.pop() else {
                break;
            };
            bytes -= page.budget();
        }
    }
    pub(crate) fn validated(&self) -> Result<Self, String> {
        if self.history.len() > MAX_HISTORY || self.bookmarks.len() > MAX_BOOKMARKS {
            return Err("Library record count exceeds limit".into());
        }
        fn checked(pages: &[Page]) -> Vec<Page> {
            let mut seen = std::collections::HashSet::new();
            pages
                .iter()
                .filter_map(|p| Page::checked(&p.uri, &p.title, p.visited))
                .filter(|p| seen.insert(p.uri.clone()))
                .collect()
        }
        let mut result = Self {
            history: checked(&self.history),
            bookmarks: checked(&self.bookmarks),
        };
        result.trim_history();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn base() -> PathBuf {
        std::env::temp_dir().join(format!(
            "paisley-library-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn records_safe_urls_only_and_sanitizes_titles() {
        let mut library = Library::default();
        for uri in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,secret",
            "https://user:password@example.org/",
            "https://user@example.org/",
            "https://example.org/\n",
            "search terms",
        ] {
            assert!(!library.visit(uri, "title", 1));
            assert!(library.bookmark(uri, "title", 1).is_err());
        }
        assert!(library.visit("https://example.org/", "a\nb\0c", 3));
        assert_eq!(library.history[0].title, "abc");
        assert_eq!(library.history[0].uri, "https://example.org/");
        assert!(!library.visit(
            &format!("https://example.org/{}", "x".repeat(2048)),
            "long",
            4
        ));
    }

    #[test]
    fn visits_deduplicate_and_bound_old_records() {
        let mut library = Library::default();
        assert!(library.visit("https://example.org/a", "A", 1));
        assert!(library.visit("https://example.org/b", "B", 2));
        assert!(library.visit("https://example.org/a", "A updated", 3));
        assert_eq!(
            library
                .history
                .iter()
                .map(|p| p.uri.as_str())
                .collect::<Vec<_>>(),
            ["https://example.org/a", "https://example.org/b"]
        );
        assert_eq!(library.history[0].visited, 3);
        for i in 0..MAX_HISTORY + 2 {
            library.visit(&format!("https://example.org/{i}"), "page", i as u64);
        }
        assert_eq!(library.history.len(), MAX_HISTORY);
        assert_eq!(library.history.last().unwrap().uri, "https://example.org/2");
    }

    #[test]
    fn late_titles_update_only_existing_visits_without_reordering() {
        let mut library = Library::default();
        library.visit("https://example.org/one", "", 10);
        library.visit("https://example.org/two", "Two", 20);
        assert!(library.update_title("https://example.org/one", "One\nupdated"));
        assert_eq!(library.history[1].title, "Oneupdated");
        assert_eq!(library.history[1].visited, 10);
        assert_eq!(library.history[0].title, "Two");
        assert!(!library.update_title("https://example.org/unvisited", "Not a visit"));
        assert_eq!(library.history.len(), 2);
    }

    #[test]
    fn bookmarks_search_and_removal_preserve_other_records() {
        let mut library = Library::default();
        library
            .bookmark("https://example.org/a", "Crab 🦀", 1)
            .unwrap();
        library.bookmark("https://example.org/b", "B", 2).unwrap();
        library
            .bookmark("https://example.org/a", "Updated CRAB 🦀", 3)
            .unwrap();
        assert_eq!(library.bookmarks.len(), 2);
        assert_eq!(library.search(true, "crab")[0].title, "Updated CRAB 🦀");
        assert_eq!(library.search(true, "example.org/b")[0].title, "B");
        library.remove_bookmark("https://example.org/a");
        assert_eq!(library.bookmarks[0].uri, "https://example.org/b");
        assert!(library.history.is_empty());
    }

    #[test]
    fn bookmark_limit_rejects_without_dropping_existing_entries() {
        let mut library = Library::default();
        for i in 0..MAX_BOOKMARKS {
            library
                .bookmark(&format!("https://example.org/{i}"), "p", 1)
                .unwrap();
        }
        let original = library.clone();
        assert!(
            library
                .bookmark("https://example.org/overflow", "p", 2)
                .is_err()
        );
        assert_eq!(library, original);
        assert!(
            library
                .bookmark("https://example.org/0", "changed", 3)
                .is_ok()
        );
    }

    #[test]
    fn persistence_isolated_private_and_corruption_preserving() {
        let root = base();
        let profile = Profile::open_at(&root, "work", false).unwrap();
        let mut library = Library::default();
        library.visit("https://example.org/visited", "Visited", 10);
        library
            .bookmark("https://example.org/saved", "Saved", 11)
            .unwrap();
        profile.save_library(&library).unwrap();
        drop(profile);
        let profile = Profile::open_at(&root, "work", false).unwrap();
        assert_eq!(profile.load_library().unwrap(), library);
        let personal = Profile::open_at(&root, "personal", false).unwrap();
        assert_eq!(personal.load_library().unwrap(), Library::default());
        let private_root = base();
        let private = Profile::open_at(&private_root, "private", true).unwrap();
        private.save_library(&library).unwrap();
        assert_eq!(private.load_library().unwrap(), Library::default());
        assert!(!private_root.exists());
        for value in ["{", "{\"version\":999,\"history\":[],\"bookmarks\":[]}"] {
            fs::write(profile.data_dir.join("library.json"), value).unwrap();
            assert!(profile.load_library().is_err());
            assert!(profile.save_library(&Library::default()).is_err());
            assert_eq!(
                fs::read_to_string(profile.data_dir.join("library.json")).unwrap(),
                value
            );
        }
        drop(profile);
        drop(personal);
        drop(private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn long_histories_fit_the_state_budget_without_losing_bookmarks() {
        let mut library = Library::default();
        let long_title = "🦀".repeat(256);
        for i in 0..MAX_BOOKMARKS {
            library
                .bookmark(
                    &format!("https://example.org/{i}/{}", "x".repeat(1900)),
                    &long_title,
                    1,
                )
                .unwrap();
        }
        for i in 0..MAX_HISTORY {
            library.visit(
                &format!("https://example.org/h/{i}/{}", "x".repeat(1900)),
                &long_title,
                1,
            );
        }
        assert_eq!(library.bookmarks.len(), MAX_BOOKMARKS);
        assert!(serde_json::to_vec(&library).unwrap().len() <= 900_000);
        assert!(!library.history.is_empty());
    }
}
