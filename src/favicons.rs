//! Profile-local, bounded favicon cache. No network requests outside WebKit.
use gtk::{gdk, gio, glib, prelude::*};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

const SITES: usize = 256;
const PIXELS: i32 = 512;
const STORED_PIXELS: i32 = 64;
const PNG_BYTES: usize = 64 * 1024;
const CONCURRENT: usize = 8;

pub struct Favicons {
    database: Option<webkit::FaviconDatabase>,
    directory: Option<PathBuf>,
    known: RefCell<HashMap<String, Vec<String>>>,
    images: RefCell<HashMap<String, gio::BytesIcon>>,
    recent: RefCell<VecDeque<String>>,
    checked: RefCell<HashMap<String, Instant>>,
    pending: RefCell<HashSet<String>>,
    queued: RefCell<VecDeque<(String, String)>>,
    refresh: RefCell<HashMap<String, String>>,
    listeners: RefCell<Vec<(String, glib::WeakRef<gtk::Stack>)>>,
    writes: RefCell<VecDeque<(String, Vec<u8>)>>,
    writing: Cell<bool>,
}

fn site(uri: &str) -> Option<String> {
    let url = url::Url::parse(uri).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.origin().ascii_serialization())
}
fn filename(origin: &str) -> String {
    format!("{:x}.png", Sha256::digest(origin.as_bytes()))
}

impl Favicons {
    pub fn new(
        database: Option<webkit::FaviconDatabase>,
        directory: Option<PathBuf>,
        pages: impl Iterator<Item = String>,
    ) -> Rc<Self> {
        let this = Rc::new(Self {
            database,
            directory,
            known: RefCell::new(HashMap::new()),
            images: RefCell::new(HashMap::new()),
            recent: RefCell::new(VecDeque::new()),
            checked: RefCell::new(HashMap::new()),
            pending: RefCell::new(HashSet::new()),
            queued: RefCell::new(VecDeque::new()),
            refresh: RefCell::new(HashMap::new()),
            listeners: RefCell::new(Vec::new()),
            writes: RefCell::new(VecDeque::new()),
            writing: Cell::new(false),
        });
        for uri in pages.take(1256) {
            this.note(&uri);
        }
        if let Some(database) = &this.database {
            let weak = Rc::downgrade(&this);
            database.connect_favicon_changed(move |_, uri, _| {
                if let Some(this) = weak.upgrade() {
                    this.request(uri, true);
                }
            });
        }
        this
    }

    fn note(&self, uri: &str) {
        let Some(origin) = site(uri) else {
            return;
        };
        if uri.len() > 2048 {
            return;
        }
        let mut known = self.known.borrow_mut();
        if !known.contains_key(&origin) && known.len() >= SITES {
            return;
        }
        let pages = known.entry(origin).or_default();
        if pages.len() < 6 && !pages.iter().any(|page| page == uri) {
            pages.push(uri.into());
        }
    }

    pub fn widget(self: &Rc<Self>, uri: &str) -> gtk::Widget {
        let stack = gtk::Stack::new();
        stack.set_size_request(28, 28);
        stack.set_valign(gtk::Align::Center);
        let domain = url::Url::parse(uri)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default();
        let initial = domain
            .trim_start_matches("www.")
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().take(1).collect::<String>())
            .unwrap_or_else(|| "•".into());
        let fallback = gtk::Label::new(Some(&initial));
        fallback.add_css_class("site-initial");
        fallback.set_tooltip_text(Some("No site icon available yet"));
        stack.add_named(&fallback, Some("initial"));
        let image = gtk::Image::new();
        image.set_pixel_size(18);
        image.set_size_request(18, 18);
        image.set_halign(gtk::Align::Center);
        image.set_valign(gtk::Align::Center);
        stack.add_named(&image, Some("icon"));
        stack.set_visible_child_name("initial");
        if let Some(origin) = site(uri) {
            if let Some(texture) = self.images.borrow().get(&origin) {
                set_image(&stack, texture);
            }
            let mut listeners = self.listeners.borrow_mut();
            listeners.retain(|(_, widget)| widget.upgrade().is_some());
            // Only mounted rows are retained, and only through weak references.
            if listeners.len() < 512 {
                listeners.push((origin, stack.downgrade()));
            }
            drop(listeners);
            self.request(uri, false);
        }
        stack.upcast()
    }

    pub fn revisit(self: &Rc<Self>, uri: &str) {
        self.request(uri, true);
    }

    fn request(self: &Rc<Self>, uri: &str, force: bool) {
        let Some(origin) = site(uri) else {
            return;
        };
        if uri.len() > 2048 {
            return;
        }
        self.note(uri);
        if self.pending.borrow().contains(&origin) {
            if force {
                self.refresh.borrow_mut().insert(origin, uri.into());
            }
            return;
        }
        if self.queued.borrow().iter().any(|(key, _)| key == &origin) {
            return;
        }
        if !force
            && self
                .checked
                .borrow()
                .get(&origin)
                .is_some_and(|time| time.elapsed() < Duration::from_secs(60))
        {
            return;
        }
        if self.pending.borrow().len() >= CONCURRENT {
            if self.queued.borrow().len() < SITES {
                self.queued.borrow_mut().push_back((origin, uri.into()));
            }
            return;
        }
        self.pending.borrow_mut().insert(origin.clone());
        let mut candidates = vec![uri.to_string()];
        if let Some(pages) = self.known.borrow().get(&origin) {
            for page in pages {
                if !candidates.contains(page) {
                    candidates.push(page.clone());
                }
            }
        }
        candidates.push(format!("{origin}/"));
        self.lookup(origin, candidates.into());
    }

    fn lookup(self: &Rc<Self>, origin: String, mut candidates: VecDeque<String>) {
        if let (Some(database), Some(uri)) = (&self.database, candidates.pop_front()) {
            let weak = Rc::downgrade(self);
            database.favicon(&uri, None::<&gio::Cancellable>, move |result| {
                if let Some(this) = weak.upgrade() {
                    match result {
                        Ok(texture) if usable(&texture) => {
                            this.remember(&origin, texture, true);
                            this.finish(&origin);
                        }
                        _ => this.lookup(origin, candidates),
                    }
                }
            });
            return;
        }
        // Already visible icons remain visible while a fresh lookup fails.
        if self.images.borrow().contains_key(&origin) {
            self.finish(&origin);
            return;
        }
        let Some(directory) = self.directory.clone() else {
            self.finish(&origin);
            return;
        };
        let path = directory.join(filename(&origin));
        let weak = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            let bytes = gio::spawn_blocking(move || read_png(&path))
                .await
                .ok()
                .flatten();
            if let Some(this) = weak.upgrade() {
                if let Some(bytes) = bytes
                    && let Ok(texture) = gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes))
                    && usable(&texture)
                {
                    this.remember(&origin, texture, false);
                }
                this.finish(&origin);
            }
        });
    }

    fn remember(self: &Rc<Self>, origin: &str, texture: gdk::Texture, persist: bool) {
        let Some(texture) = normalize(texture) else {
            return;
        };
        let bytes = texture.save_to_png_bytes();
        if bytes.len() > PNG_BYTES {
            return;
        }
        let icon = gio::BytesIcon::new(&bytes);
        let mut recent = self.recent.borrow_mut();
        recent.retain(|key| key != origin);
        recent.push_back(origin.into());
        self.images.borrow_mut().insert(origin.into(), icon.clone());
        while recent.len() > SITES {
            if let Some(old) = recent.pop_front() {
                self.images.borrow_mut().remove(&old);
            }
        }
        self.listeners.borrow_mut().retain(|(key, widget)| {
            let Some(widget) = widget.upgrade() else {
                return false;
            };
            if key == origin {
                set_image(&widget, &icon);
            }
            true
        });
        if persist && self.directory.is_some() {
            if bytes.len() <= PNG_BYTES {
                let name = filename(origin);
                let mut writes = self.writes.borrow_mut();
                writes.retain(|(key, _)| key != &name);
                if writes.len() < 64 {
                    writes.push_back((name, bytes.as_ref().to_vec()));
                }
                drop(writes);
                self.write_next();
            }
        }
    }

    fn write_next(self: &Rc<Self>) {
        if self.writing.get() {
            return;
        }
        let Some(directory) = self.directory.clone() else {
            return;
        };
        let next = self.writes.borrow_mut().pop_front();
        let Some((name, bytes)) = next else {
            return;
        };
        self.writing.set(true);
        let weak = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            let _ = gio::spawn_blocking(move || write_png(&directory, &name, &bytes)).await;
            if let Some(this) = weak.upgrade() {
                this.writing.set(false);
                this.write_next();
            }
        });
    }

    fn finish(self: &Rc<Self>, origin: &str) {
        self.pending.borrow_mut().remove(origin);
        let mut checked = self.checked.borrow_mut();
        if checked.len() >= SITES {
            checked.retain(|_, time| time.elapsed() < Duration::from_secs(60));
        }
        if checked.len() < SITES {
            checked.insert(origin.into(), Instant::now());
        }
        drop(checked);
        let refresh = self.refresh.borrow_mut().remove(origin);
        if let Some(uri) = refresh {
            self.request(&uri, true);
        }
        let next = if self.pending.borrow().len() < CONCURRENT {
            self.queued.borrow_mut().pop_front()
        } else {
            None
        };
        if let Some((_, uri)) = next {
            self.request(&uri, true);
        }
    }
}
fn normalize(texture: gdk::Texture) -> Option<gdk::Texture> {
    if !usable(&texture) {
        return None;
    }
    let largest = texture.width().max(texture.height());
    if largest <= STORED_PIXELS {
        return Some(texture);
    }
    let pixbuf = gdk::pixbuf_get_from_texture(&texture)?;
    let small = pixbuf.scale_simple(
        (texture.width() * STORED_PIXELS / largest).max(1),
        (texture.height() * STORED_PIXELS / largest).max(1),
        gtk::gdk_pixbuf::InterpType::Bilinear,
    )?;
    Some(gdk::Texture::for_pixbuf(&small))
}

fn usable(texture: &gdk::Texture) -> bool {
    texture.width() > 0
        && texture.height() > 0
        && texture.width() <= PIXELS
        && texture.height() <= PIXELS
}
fn set_image(stack: &gtk::Stack, icon: &gio::BytesIcon) {
    if let Some(image) = stack.child_by_name("icon").and_downcast::<gtk::Image>() {
        image.set_from_gicon(icon);
        stack.set_visible_child_name("icon");
    }
}
fn read_png(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(PNG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > PNG_BYTES
        || bytes.len() < 24
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || &bytes[12..16] != b"IHDR"
    {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (width > 0 && height > 0 && width <= STORED_PIXELS as u32 && height <= STORED_PIXELS as u32)
        .then_some(bytes)
}
fn write_png(directory: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::{
        fs,
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    };
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = directory.join(format!(".icon-{}-{stamp}", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(bytes)?;
        fs::rename(&temporary, directory.join(name))?;
        let mut files: Vec<_> = fs::read_dir(directory)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                let name = name.to_str()?;
                if name.len() != 68
                    || !name.ends_with(".png")
                    || !name[..64].bytes().all(|c| c.is_ascii_hexdigit())
                {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                metadata
                    .is_file()
                    .then(|| (metadata.modified().ok(), entry.path()))
            })
            .collect();
        files.sort_by_key(|(modified, _)| *modified);
        let excess = files.len().saturating_sub(SITES);
        for (_, path) in files.into_iter().take(excess) {
            let _ = fs::remove_file(path);
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
