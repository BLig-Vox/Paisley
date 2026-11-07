use browny::profile::Profile;
#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

pub const STARTER_RULES: &str = include_str!("../assets/blocking.json");

pub struct Blocker {
    store: webkit::UserContentFilterStore,
    filter: RefCell<Option<webkit::UserContentFilter>>,
    pub ready: Cell<bool>,
    pub error: RefCell<Option<String>>,
    scratch: Option<PathBuf>,
}
impl Blocker {
    pub fn new(profile: &Profile) -> Result<Rc<Self>, String> {
        let scratch = if profile.private {
            let name = format!(
                "browny-filters-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(name);
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            builder.create(&path).map_err(|e| e.to_string())?;
            Some(path)
        } else {
            None
        };
        let path = scratch
            .clone()
            .unwrap_or_else(|| profile.cache_dir.join("content-filters"));
        let store = webkit::UserContentFilterStore::new(
            path.to_str().ok_or("Filter cache path is not UTF-8")?,
        );
        Ok(Rc::new(Self {
            store,
            filter: RefCell::new(None),
            ready: Cell::new(false),
            error: RefCell::new(None),
            scratch,
        }))
    }
    pub fn filter(&self) -> Option<webkit::UserContentFilter> {
        self.filter.borrow().clone()
    }
    pub fn compile(self: &Rc<Self>, rules: &str, done: impl FnOnce() + 'static) {
        self.ready.set(false);
        let this = Rc::clone(self);
        use std::hash::{Hash, Hasher};
        let mut digest = std::collections::hash_map::DefaultHasher::new();
        rules.hash(&mut digest);
        let identifier = format!("browny-{:x}", digest.finish());
        self.store.save(
            &identifier,
            &gtk::glib::Bytes::from_owned(rules.as_bytes().to_vec()),
            None::<&gtk::gio::Cancellable>,
            move |result| {
                match result {
                    Ok(filter) => {
                        *this.filter.borrow_mut() = Some(filter);
                        *this.error.borrow_mut() = None;
                    }
                    Err(error) => {
                        *this.error.borrow_mut() = Some(error.to_string());
                    }
                }
                this.ready.set(true);
                done();
            },
        );
    }
}
impl Drop for Blocker {
    fn drop(&mut self) {
        if let Some(path) = &self.scratch {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
