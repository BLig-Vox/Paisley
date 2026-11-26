//! Profile-local, explicitly installed page scripts. They execute with page privileges.
use std::path::Path;

pub fn scripts(profile: &Path, uri: &str) -> Vec<String> {
    let Ok(url) = url::Url::parse(uri) else {
        return vec![];
    };
    if !matches!(url.scheme(), "http" | "https") {
        return vec![];
    }
    let Some(host) = url.host_str() else {
        return vec![];
    };
    let root = profile.join("native-extensions").join(host);
    let Ok(entries) = std::fs::read_dir(root) else {
        return vec![];
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    paths.sort();
    paths.into_iter().filter_map(|path| {
        let js = std::fs::read_to_string(path.join("index.js")).ok()?;
        let css = std::fs::read_to_string(path.join("style.css")).unwrap_or_default();
        let expected = serde_json::to_string(host).ok()?;
        let css = serde_json::to_string(&css).ok()?;
        Some(format!("if ((location.protocol === 'https:' || location.protocol === 'http:') && location.hostname === {expected}) {{ (() => {{ const css = {css}; if (css) {{ const style = document.createElement('style'); style.textContent = css; (document.head || document.documentElement).appendChild(style); }}\n{js}\n}})(); }}"))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_http_and_other_hosts() {
        let root = std::env::temp_dir().join(format!(
            "paisley-native-extension-test-{}",
            std::process::id()
        ));
        let dir = root.join("native-extensions/example.com/demo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.js"), "document.title='native test';").unwrap();
        std::fs::write(dir.join("style.css"), "body { color: red; }").unwrap();
        assert!(scripts(&root, "https://other.example/").is_empty());
        assert!(scripts(&root, "file:///example.com").is_empty());
        let loaded = scripts(&root, "https://example.com/page");
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].contains("document.title='native test'"));
        assert!(loaded[0].contains("body { color: red; }"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
