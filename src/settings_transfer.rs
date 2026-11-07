//! Bounded, explicit settings interchange; exports omit site-identifying data.
use crate::profile::Settings;

pub const MAX_IMPORT_BYTES: usize = 64 * 1024;

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transfer {
    version: u32,
    settings: Patch,
}

// A missing member means retain the current value; an explicit null is invalid.
fn present<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    restore_session: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    remember_history: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    show_memory: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    block_trackers: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    strict_privacy: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    homepage: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    blocking_exceptions: Option<Vec<String>>,
}

pub fn load_file(path: &std::path::Path, current: &Settings) -> Result<Settings, String> {
    import_preferences(
        &crate::transfer_io::read_bounded(path, MAX_IMPORT_BYTES)?,
        current,
    )
}

pub fn save_export(path: &std::path::Path, current: &Settings) -> Result<(), String> {
    crate::transfer_io::write_new(path, &export_preferences(current)?)
}

pub fn export_preferences(settings: &Settings) -> Result<Vec<u8>, String> {
    let transfer = Transfer {
        version: 1,
        settings: Patch {
            restore_session: Some(settings.restore_session),
            remember_history: Some(settings.remember_history),
            show_memory: Some(settings.show_memory),
            block_trackers: Some(settings.block_trackers),
            strict_privacy: Some(settings.strict_privacy),
            ..Patch::default()
        },
    };
    let mut bytes = serde_json::to_vec_pretty(&transfer)
        .map_err(|_| "Cannot serialize supported preferences")?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn import_preferences(bytes: &[u8], current: &Settings) -> Result<Settings, String> {
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err("Settings import exceeds 64 KiB".into());
    }
    let Transfer { version, settings: patch } = serde_json::from_slice(bytes).map_err(|error| format!("Unsupported settings schema at line {}, column {}; expected supported typed preferences only", error.line(), error.column()))?;
    if version != 1 {
        return Err("Unsupported settings transfer version; original file is unchanged".into());
    }
    let mut next = current.clone();
    if let Some(value) = patch.restore_session {
        next.restore_session = value;
    }
    if let Some(value) = patch.remember_history {
        next.remember_history = value;
    }
    if let Some(value) = patch.show_memory {
        next.show_memory = value;
    }
    if let Some(value) = patch.block_trackers {
        next.block_trackers = value;
    }
    if let Some(value) = patch.strict_privacy {
        next.strict_privacy = value;
    }
    if let Some(value) = patch.homepage {
        if value.len() > 2048 || !crate::profile::valid_url(&value) {
            return Err(
                "Imported homepage must be a credential-free HTTP(S) URL of at most 2048 bytes"
                    .into(),
            );
        }
        next.homepage = value;
    }
    if let Some(values) = patch.blocking_exceptions {
        if values.len() > 128 {
            return Err("Import contains more than 128 site exceptions".into());
        }
        let mut seen = std::collections::HashSet::new();
        for value in &values {
            if value.len() > 2048
                || !crate::profile::valid_url(value)
                || url::Url::parse(value)
                    .ok()
                    .is_none_or(|url| url.origin().ascii_serialization() != *value)
                || !seen.insert(value)
            {
                return Err("Site exceptions must be unique canonical HTTP(S) origins, without paths, credentials, query or fragment".into());
            }
        }
        next.blocking_exceptions = values;
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            use std::os::unix::fs::DirBuilderExt;
            let root = std::env::temp_dir().join(format!(
                "browny-settings-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&root)
                .unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn file_export_is_private_no_overwrite_and_import_is_bounded_regular_only() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
        let fixture = Fixture::new();
        let output = fixture.0.join("preferences.json");
        let current = Settings {
            remember_history: false,
            ..Settings::default()
        };
        save_export(&output, &current).unwrap();
        let original = std::fs::read(&output).unwrap();
        assert_eq!(
            std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::metadata(&output).unwrap().nlink(), 1);
        assert!(save_export(&output, &Settings::default()).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), original);
        assert_eq!(load_file(&output, &Settings::default()).unwrap(), current);
        let link = fixture.0.join("symlink");
        symlink(&output, &link).unwrap();
        assert!(load_file(&link, &current).is_err());
        assert!(save_export(&link, &current).is_err());
        assert!(load_file(&fixture.0, &current).is_err());
        let oversized = fixture.0.join("large.json");
        std::fs::write(&oversized, vec![b' '; MAX_IMPORT_BYTES + 1]).unwrap();
        assert!(load_file(&oversized, &current).is_err());
        assert!(std::fs::read_dir(&fixture.0).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".browny-transfer-")
        }));
    }

    #[test]
    fn export_omits_homepage_site_exceptions_paths_and_unsupported_agent_option() {
        let current = Settings {
            homepage: "https://example.test/?token=owned-dummy#fragment".into(),
            blocking_exceptions: vec!["https://site.example.test".into()],
            download_directory: Some("/owned/dummy/path".into()),
            agent_enabled: true,
            remember_history: false,
            ..Settings::default()
        };
        let bytes = export_preferences(&current).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        for absent in [
            "homepage",
            "blocking_exceptions",
            "example.test",
            "owned",
            "agent_enabled",
            "download_directory",
        ] {
            assert!(!text.contains(absent), "export included {absent}");
        }
        let other = Settings {
            remember_history: true,
            ..current.clone()
        };
        assert_eq!(import_preferences(&bytes, &other).unwrap(), current);
    }

    #[test]
    fn import_applies_supported_typed_patch_without_resetting_omitted_fields() {
        let current = Settings {
            homepage: "https://kept.example.test/".into(),
            remember_history: false,
            ..Settings::default()
        };
        let result = import_preferences(
            br#"{"version":1,"settings":{"show_memory":false,"strict_privacy":true}}"#,
            &current,
        )
        .unwrap();
        assert!(!result.show_memory);
        assert!(result.strict_privacy);
        assert_eq!(result.homepage, current.homepage);
        assert!(!result.remember_history);
        assert_eq!(
            current,
            Settings {
                homepage: "https://kept.example.test/".into(),
                remember_history: false,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn import_validates_homepage_and_exact_bounded_origins() {
        let result = import_preferences(br#"{"version":1,"settings":{"homepage":"https://home.example.test/","blocking_exceptions":["https://site.example.test"]}}"#, &Settings::default()).unwrap();
        assert_eq!(result.homepage, "https://home.example.test/");
        assert_eq!(result.blocking_exceptions, ["https://site.example.test"]);
        for uri in [
            "file:///tmp/owned",
            "https://user:password@example.test/",
            "javascript:alert(1)",
        ] {
            let bytes =
                serde_json::to_vec(&serde_json::json!({"version":1,"settings":{"homepage":uri}}))
                    .unwrap();
            assert!(import_preferences(&bytes, &Settings::default()).is_err());
        }
        for origin in [
            "https://example.test/",
            "https://example.test/path",
            "https://example.test:443",
            "https://example.test?secret=dummy",
            "https://user@example.test",
            "https://example.test#x",
        ] {
            let bytes = serde_json::to_vec(
                &serde_json::json!({"version":1,"settings":{"blocking_exceptions":[origin]}}),
            )
            .unwrap();
            assert!(
                import_preferences(&bytes, &Settings::default()).is_err(),
                "accepted noncanonical origin: {origin}"
            );
        }
    }

    #[test]
    fn import_rejects_unknown_future_duplicate_wrong_type_and_oversized_records() {
        for text in [
            r#"{"version":2,"settings":{}}"#,
            r#"{"version":1,"settings":{},"unknown":true}"#,
            r#"{"version":1,"settings":{"unknown":true}}"#,
            r#"{"version":1,"settings":{"show_memory":"yes"}}"#,
            r#"{"version":1,"settings":{"agent_enabled":true}}"#,
            r#"{"version":1,"settings":{"download_directory":"/tmp/owned"}}"#,
            r#"{"version":1,"settings":{"show_memory":true,"show_memory":false}}"#,
            r#"{"version":1,"settings":{"blocking_exceptions":["https://x.test","https://x.test"]}}"#,
            r#"{"version":1,"settings":{"show_memory":null}}"#,
        ] {
            assert!(
                import_preferences(text.as_bytes(), &Settings::default()).is_err(),
                "accepted: {text}"
            );
        }
        assert!(
            import_preferences(&vec![b' '; MAX_IMPORT_BYTES + 1], &Settings::default()).is_err()
        );
        let origins: Vec<_> = (0..129)
            .map(|i| format!("https://site{i}.example.test"))
            .collect();
        let bytes = serde_json::to_vec(
            &serde_json::json!({"version":1,"settings":{"blocking_exceptions":origins}}),
        )
        .unwrap();
        assert!(import_preferences(&bytes, &Settings::default()).is_err());
    }
}
