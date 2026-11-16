//! Discovery and closed-profile maintenance. Same-user filesystem races are not
//! a security boundary; deletion across XDG roots can fail partway through.
use crate::profile::{self, Profile};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

const MAX_PROFILES: usize = 256;
const MAX_NAME: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEntry {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    name: String,
}

#[derive(Clone)]
pub struct ProfileCatalog {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    private: bool,
}

impl ProfileCatalog {
    pub fn system(private: bool) -> Result<Self, String> {
        Ok(Self {
            // Retain Browny storage roots so the rename keeps existing profiles.
            config: profile::xdg("XDG_CONFIG_HOME", ".config")?.join("browny/profiles"),
            data: profile::xdg("XDG_DATA_HOME", ".local/share")?.join("browny/profiles"),
            cache: profile::xdg("XDG_CACHE_HOME", ".cache")?.join("browny/profiles"),
            private,
        })
    }
    pub fn at(base: &Path, private: bool) -> Self {
        Self {
            config: base.to_owned(),
            data: base.to_owned(),
            cache: base.to_owned(),
            private,
        }
    }
    fn roots(&self) -> [&Path; 3] {
        [&self.config, &self.data, &self.cache]
    }
    fn check_path(path: &Path) -> Result<(), String> {
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err("Invalid profile root".into());
        }
        let mut prefix = PathBuf::new();
        for c in path.components() {
            prefix.push(c.as_os_str());
            profile::reject_symlink(&prefix)?;
        }
        Ok(())
    }
    pub fn display_name(&self, id: &str) -> Result<String, String> {
        Profile::validate_id(id)?;
        if self.private {
            return Ok(id.to_owned());
        }
        self.name(id)
    }
    fn name(&self, id: &str) -> Result<String, String> {
        let path = self.config.join(id).join("profile.json");
        Self::check_path(&path)?;
        match profile::read_record::<Metadata>(&path)? {
            Some(m) => {
                Self::check_name(&m.name)?;
                Ok(m.name)
            }
            None => Ok(id.to_owned()),
        }
    }
    fn check_name(name: &str) -> Result<(), String> {
        if name.trim() != name
            || name.is_empty()
            || name.len() > MAX_NAME
            || name.chars().any(char::is_control)
        {
            return Err("Display name must be 1–128 bytes without surrounding whitespace or control characters".into());
        }
        Ok(())
    }
    pub fn list(&self) -> Result<Vec<ProfileEntry>, String> {
        if self.private {
            return Ok(Vec::new());
        }
        let mut ids = BTreeSet::new();
        let mut scanned = 0usize;
        for root in self.roots() {
            Self::check_path(root)?;
            let entries = match fs::read_dir(root) {
                Ok(e) => e,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("{}: {e}", root.display())),
            };
            for entry in entries {
                scanned += 1;
                if scanned > 3072 {
                    return Err("Too many directory entries to discover profiles".into());
                }
                let entry = entry.map_err(|e| e.to_string())?;
                let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if Profile::validate_id(&id).is_err() {
                    continue;
                }
                let kind = entry.file_type().map_err(|e| e.to_string())?;
                if kind.is_symlink() {
                    return Err(format!("Symlink not allowed: {}", entry.path().display()));
                }
                if !kind.is_dir() {
                    continue;
                }
                ids.insert(id);
                if ids.len() > MAX_PROFILES {
                    return Err("Too many profiles to display".into());
                }
            }
        }
        ids.into_iter()
            .map(|id| {
                Ok(ProfileEntry {
                    name: self.name(&id)?,
                    id,
                })
            })
            .collect()
    }
    fn locked(&self, id: &str) -> Result<(fs::File, fs::File), String> {
        if self.private {
            return Err("Profile management is unavailable in private mode".into());
        }
        Profile::validate_id(id)?;
        for root in self.roots() {
            Self::check_path(&root.join(id))?;
        }
        let dir = self.data.join(id);
        if !dir.is_dir() {
            return Err("Profile data directory does not exist".into());
        }
        let maintenance = profile::maintenance_lock(&self.data)?;
        let path = dir.join("profile.lock");
        profile::reject_symlink(&path)?;
        let file = profile::open_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Lock is not a regular file".into());
        }
        file.try_lock()
            .map_err(|e| format!("Profile is locked: {e}"))?;
        Ok((maintenance, file))
    }
    pub fn rename(&self, id: &str, name: &str) -> Result<(), String> {
        Self::check_name(name)?;
        let _lock = self.locked(id)?;
        let _ = self.name(id)?;
        let dir = self.config.join(id);
        profile::ensure_dir(&dir)?;
        profile::write_record(&dir.join("profile.json"), &Metadata { name: name.into() })
    }
    pub fn delete(&self, id: &str, confirmed: bool) -> Result<(), String> {
        if !confirmed {
            return Err("Deletion requires confirmation".into());
        }
        Profile::validate_id(id)?;
        for target in self.roots().map(|root| root.join(id)) {
            if self.roots().iter().any(|root| root.starts_with(&target)) {
                return Err("Cannot delete with nested profile roots: another profile storage root is inside this profile".into());
            }
        }
        let _lock = self.locked(id)?;
        let _ = self.name(id)?;
        // Remove data last so the lock remains reachable for as long as possible.
        let data = self.data.join(id);
        let mut paths = Vec::new();
        for path in [self.config.join(id), self.cache.join(id)] {
            if path != data && !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths.push(data);
        for path in &paths {
            Self::check_path(path)?;
        }
        for path in paths {
            match fs::remove_dir_all(&path) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => {
                    return Err(format!(
                        "Could not remove {}: {e}. Some profile data may already have been deleted.",
                        path.display()
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_and_metadata_are_bounded_and_invalid_names_do_not_write() {
        let base = std::env::temp_dir().join(format!(
            "paisley-catalog-bounds-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        drop(Profile::open_at(&base, "work", false).unwrap());
        let catalog = ProfileCatalog::at(&base, false);
        for name in ["", " leading", "trailing ", "line\nbreak", &"x".repeat(129)] {
            assert!(catalog.rename("work", name).is_err());
            assert!(!base.join("work/profile.json").exists());
        }
        let metadata = base.join("work/profile.json");
        fs::write(&metadata, vec![b' '; 1024 * 1024 + 1]).unwrap();
        assert!(catalog.list().is_err());
        assert!(catalog.rename("work", "New label").is_err());
        assert!(catalog.delete("work", true).is_err());
        assert_eq!(fs::metadata(&metadata).unwrap().len(), 1024 * 1024 + 1);
        fs::remove_file(metadata).unwrap();
        for index in 0..256 {
            fs::create_dir(base.join(format!("p{index}"))).unwrap();
        }
        assert!(
            catalog.list().is_err(),
            "257 profiles exceeded the discovery bound"
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn deletion_refuses_nested_roots_that_would_remove_another_profile() {
        let base = std::env::temp_dir().join(format!(
            "paisley-catalog-nesting-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = base.join("profiles");
        let data = config.join("work/nested-data-root");
        let cache = base.join("cache");
        fs::create_dir_all(data.join("work")).unwrap();
        fs::create_dir_all(data.join("other")).unwrap();
        let marker = data.join("other/owned-marker");
        fs::write(&marker, b"keep other profile").unwrap();
        let catalog = ProfileCatalog {
            config,
            data,
            cache,
            private: false,
        };
        let result = catalog.delete("work", true);
        let preserved = marker.exists();
        fs::remove_dir_all(base).unwrap();
        assert!(
            result.is_err(),
            "overlapping roots were accepted for deletion"
        );
        assert!(
            preserved,
            "deleting one profile removed another profile's data"
        );
    }
}
