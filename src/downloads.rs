use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use browny::download_history::{DownloadHistory, DownloadRecord, Outcome};
use gtk::prelude::*;
use webkit::prelude::*;

struct Item {
    row: gtk::Box,
    reveal: gtk::Button,
    received: u64,
    label: gtk::Label,
    progress: gtk::ProgressBar,
    cancel: gtk::Button,
    retry: gtk::Button,
    download: Option<webkit::Download>,
    source_uri: Option<String>,
    source_view: Option<gtk::glib::WeakRef<webkit::WebView>>,
    path: Option<PathBuf>,
    status: String,
    chooser: gtk::gio::Cancellable,
}

pub struct Downloads {
    parent: gtk::ApplicationWindow,
    window: gtk::Window,
    list: gtk::Box,
    saved_list: gtk::Box,
    message: gtk::Label,
    storage_warning: gtk::Label,
    storage_enabled: std::cell::Cell<bool>,
    history: RefCell<DownloadHistory>,
    previous: RefCell<Vec<DownloadRecord>>,
    changed: RefCell<Option<Box<dyn Fn()>>>,
    items: RefCell<Vec<Item>>,
    completed: RefCell<Vec<PathBuf>>,
    errors: RefCell<Vec<String>>,
    test_directory: Option<PathBuf>,
    self_weak: RefCell<std::rc::Weak<Downloads>>,
}

impl Downloads {
    #[cfg(test)]
    pub fn new(parent: &gtk::ApplicationWindow) -> Rc<Self> {
        Self::build(parent, None, DownloadHistory::default(), true)
    }

    pub fn for_profile(
        parent: &gtk::ApplicationWindow,
        history: DownloadHistory,
        private: bool,
    ) -> Rc<Self> {
        Self::build(parent, None, history, private)
    }

    pub fn on_changed(&self, changed: impl Fn() + 'static) {
        *self.changed.borrow_mut() = Some(Box::new(changed));
    }

    fn history_changed(&self) {
        if let Some(callback) = self.changed.borrow().as_ref() {
            callback();
        }
    }

    #[cfg(test)]
    pub fn with_directory(parent: &gtk::ApplicationWindow, path: PathBuf) -> Rc<Self> {
        Self::build(parent, Some(path), DownloadHistory::default(), true)
    }

    #[cfg(test)]
    pub fn with_directory_history(
        parent: &gtk::ApplicationWindow,
        path: PathBuf,
        history: DownloadHistory,
    ) -> Rc<Self> {
        Self::build(parent, Some(path), history, false)
    }

    fn build(
        parent: &gtk::ApplicationWindow,
        test_directory: Option<PathBuf>,
        history: DownloadHistory,
        private: bool,
    ) -> Rc<Self> {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let saved_list = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 12);
        rows.append(&list);
        rows.append(&gtk::Label::new(Some(
            "Saved records from previous sessions",
        )));
        rows.append(&saved_list);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&rows)
            .vexpand(true)
            .hexpand(true)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);
        let notice = if private {
            "Private downloads: records stay in memory; files you explicitly save remain on disk. Files never auto-open."
        } else {
            "Up to 200 terminal download records (source URLs, paths and byte counts) are saved in this profile, unencrypted. Older records may be evicted earlier for size. Files never auto-open."
        };
        content.append(
            &gtk::Label::builder()
                .label(notice)
                .wrap(true)
                .xalign(0.0)
                .build(),
        );
        let storage_warning = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .visible(false)
            .build();
        storage_warning.add_css_class("error");
        content.append(&storage_warning);
        let clear = gtk::Button::with_label("Clear download records…");
        content.append(&clear);
        let message = gtk::Label::builder().wrap(true).xalign(0.0).build();
        message.add_css_class("error");
        content.append(&message);
        content.append(&scroller);
        let window = gtk::Window::builder()
            .title("Downloads")
            .default_width(560)
            .default_height(420)
            .transient_for(parent)
            .hide_on_close(true)
            .child(&content)
            .build();
        let this = Rc::new_cyclic(|weak| Self {
            parent: parent.clone(),
            window,
            list,
            saved_list,
            message,
            storage_warning,
            storage_enabled: std::cell::Cell::new(true),
            previous: RefCell::new(history.records.clone()),
            history: RefCell::new(history),
            changed: RefCell::new(None),
            items: RefCell::new(Vec::new()),
            completed: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
            test_directory,
            self_weak: RefCell::new(weak.clone()),
        });
        let weak = Rc::downgrade(&this);
        clear.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else { return; };
            let dialog = gtk::AlertDialog::builder().message("Clear download records?")
                .detail("Remove terminal download metadata from this profile. Saved files are not deleted; active downloads continue and will create a record when they finish.")
                .buttons(["Cancel", "Clear records"]).cancel_button(0).default_button(0).modal(true).build();
            let weak = Rc::downgrade(&this);
            dialog.choose(Some(&this.window), None::<&gtk::gio::Cancellable>, move |choice| {
                if choice != Ok(1) { return; }
                if let Some(this) = weak.upgrade() {
                    this.history.borrow_mut().records.clear();
                    this.previous.borrow_mut().clear();
                    for item in this.items.borrow().iter() {
                        if matches!(item.status.as_str(), "Completed" | "Failed" | "Cancelled") { item.row.set_visible(false); }
                    }
                    this.render_saved();
                    this.history_changed();
                }
            });
        });
        this.render_saved();
        this
    }

    pub fn snapshot(&self) -> DownloadHistory {
        self.history.borrow().clone()
    }

    pub fn use_memory_records(&self, reason: &str) {
        self.storage_enabled.set(false);
        self.storage_warning.set_text(&format!("Saved download records are unavailable: {reason}. The existing file is preserved. New records and clearing are memory-only until the file is explicitly repaired and Browny is restarted."));
        self.storage_warning.set_visible(true);
    }

    pub fn storage_available(&self) -> bool {
        self.storage_enabled.get()
    }

    pub fn storage_snapshot(&self) -> Option<DownloadHistory> {
        self.storage_enabled.get().then(|| self.snapshot())
    }

    fn render_saved(&self) {
        while let Some(child) = self.saved_list.first_child() {
            self.saved_list.remove(&child);
        }
        for record in self.previous.borrow().iter() {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let text = format!(
                "{:?} · {} · Unix time {}\n{}\n{}",
                record.outcome,
                format_size(record.received),
                record.finished,
                record
                    .destination
                    .as_deref()
                    .unwrap_or("No destination selected"),
                record.source.as_deref().unwrap_or("Source not retained")
            );
            row.append(
                &gtk::Label::builder()
                    .label(text)
                    .wrap(true)
                    .xalign(0.0)
                    .build(),
            );
            if record.outcome == Outcome::Completed
                && let Some(path) = &record.destination
            {
                let reveal = gtk::Button::with_label("Reveal in folder");
                let path = PathBuf::from(path);
                let weak = self.weak_self();
                reveal.connect_clicked(move |_| {
                    if let Some(this) = weak.upgrade() {
                        this.reveal(&path);
                    }
                });
                row.append(&reveal);
            }
            self.saved_list.append(&row);
        }
    }

    fn reveal(&self, path: &Path) {
        let path = match checked_reveal_path(path) {
            Ok(path) => path,
            Err(error) => {
                self.message.set_text(&format!("Cannot reveal: {error}"));
                return;
            }
        };
        let launcher = gtk::FileLauncher::new(Some(&gtk::gio::File::for_path(path)));
        let weak = self.weak_self();
        launcher.open_containing_folder(
            Some(&self.window),
            None::<&gtk::gio::Cancellable>,
            move |result| {
                if let Some(this) = weak.upgrade() {
                    match result {
                        Ok(()) => this.message.set_text(""),
                        Err(error) => this.message.set_text(&format!("Cannot reveal: {error}")),
                    }
                }
            },
        );
    }

    fn record_terminal(&self, index: usize, outcome: Outcome) {
        let changed = {
            let items = self.items.borrow();
            let item = &items[index];
            let finished = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            self.history.borrow_mut().record(
                item.source_uri.as_deref(),
                item.path.as_deref(),
                outcome,
                item.received,
                finished,
            )
        };
        if changed {
            // Disk retention and the saved-record UI must describe the same set.
            let retained = self.history.borrow();
            self.previous
                .borrow_mut()
                .retain(|record| retained.records.contains(record));
            drop(retained);
            self.render_saved();
            self.history_changed();
        }
    }

    pub fn show(&self) {
        self.window.present();
    }

    #[cfg(test)]
    pub fn completed_paths(&self) -> Vec<PathBuf> {
        self.completed.borrow().clone()
    }
    #[cfg(test)]
    pub fn errors(&self) -> Vec<String> {
        self.errors.borrow().clone()
    }
    #[cfg(test)]
    pub fn active_count(&self) -> usize {
        self.items
            .borrow()
            .iter()
            .filter(|item| {
                matches!(
                    item.status.as_str(),
                    "Starting" | "Waiting for location" | "Downloading"
                )
            })
            .count()
    }

    fn prepare(self: &Rc<Self>, download: &webkit::Download) -> (usize, String) {
        download.set_allow_overwrite(false);
        let source_uri = download
            .request()
            .and_then(|request| request.uri())
            .map(|u| u.to_string());
        let suggested = download
            .response()
            .and_then(|response| response.suggested_filename())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "download".into());
        let filename = safe_basename(&suggested);
        let status = if download.response().is_some() {
            "Waiting for location"
        } else {
            "Starting"
        };
        let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = gtk::Label::builder()
            .label(format!("{filename} — {status}"))
            .xalign(0.0)
            .hexpand(true)
            .build();
        let reveal = gtk::Button::with_label("Reveal in folder");
        reveal.set_visible(false);
        header.append(&reveal);
        let cancel = gtk::Button::with_label("Cancel");
        let retry = gtk::Button::with_label("Retry");
        retry.set_visible(false);
        header.append(&label);
        header.append(&retry);
        header.append(&cancel);
        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        row.append(&header);
        row.append(&progress);
        self.list.append(&row);
        let index = self.items.borrow().len();
        self.items.borrow_mut().push(Item {
            row,
            reveal,
            received: 0,
            label,
            progress,
            cancel,
            retry,
            download: Some(download.clone()),
            source_uri,
            source_view: download.web_view().map(|view| view.downgrade()),
            path: None,
            status: status.into(),
            chooser: gtk::gio::Cancellable::new(),
        });

        let weak = Rc::downgrade(self);
        self.items.borrow()[index].reveal.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let path = this
                    .items
                    .borrow()
                    .get(index)
                    .and_then(|item| item.path.clone());
                if let Some(path) = path {
                    this.reveal(&path);
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.items.borrow()[index].cancel.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.cancel_item(index);
            }
        });
        let weak = Rc::downgrade(self);
        self.items.borrow()[index].retry.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.retry_item(index);
            }
        });
        // Observe terminal events before presenting an asynchronous chooser.
        let weak = self.weak_self();
        download.connect_received_data(move |dl, _| {
            if let Some(this) = weak.upgrade() {
                this.update_progress(index, dl);
            }
        });
        let weak = self.weak_self();
        download.connect_finished(move |_| {
            if let Some(this) = weak.upgrade() {
                this.finish(index);
            }
        });
        let weak = self.weak_self();
        download.connect_failed(move |_, error| {
            if let Some(this) = weak.upgrade() {
                this.fail(index, format!("Download failed: {error}"));
            }
        });
        (index, filename)
    }

    pub fn start(self: &Rc<Self>, download: &webkit::Download) {
        // Track immediately, not only after a response/chooser: startup can fail.
        let (index, _) = self.prepare(download);
        let weak = Rc::downgrade(self);
        download.connect_decide_destination(move |download, suggested| {
            if let Some(this) = weak.upgrade() {
                let filename = safe_basename(suggested);
                {
                    let mut items = this.items.borrow_mut();
                    let item = &mut items[index];
                    if !matches!(item.status.as_str(), "Starting" | "Waiting for location")
                        || item.download.as_ref() != Some(download)
                    {
                        return true;
                    }
                    item.status = "Waiting for location".into();
                    item.label
                        .set_text(&format!("{filename} — Waiting for location"));
                }
                this.choose_destination(index, download, filename);
            } else {
                download.cancel();
            }
            true
        });
    }

    fn choose_destination(
        self: &Rc<Self>,
        index: usize,
        download: &webkit::Download,
        filename: String,
    ) {
        if let Some(directory) = &self.test_directory {
            match fresh_file_path(directory, &filename) {
                Ok(path) => self.set_destination(index, download, path),
                Err(error) => {
                    self.fail(index, error);
                    download.cancel();
                }
            }
            return;
        }

        let dialog = gtk::FileDialog::new();
        dialog.set_title("Save download");
        dialog.set_initial_name(Some(&filename));
        let weak = Rc::downgrade(self);
        let download = download.clone();
        let chooser = self.items.borrow()[index].chooser.clone();
        dialog.save(Some(&self.parent), Some(&chooser), move |result| {
            let Some(this) = weak.upgrade() else {
                download.cancel();
                return;
            };
            if !this.waiting_for_destination(index, &download) {
                return;
            }
            match result {
                Ok(file) => match file.path() {
                    Some(path) => match canonical_destination(&path) {
                        Ok(path) => match ensure_destination_is_fresh(&path) {
                            Ok(()) => this.set_destination(index, &download, path),
                            Err(error) => {
                                this.fail(index, error);
                                download.cancel();
                            }
                        },
                        Err(error) => {
                            this.fail(index, error);
                            download.cancel();
                        }
                    },
                    None => {
                        this.fail(
                            index,
                            "The selected destination is not a local file.".into(),
                        );
                        download.cancel();
                    }
                },
                Err(error)
                    if error.matches(gtk::gio::IOErrorEnum::Cancelled)
                        || error.matches(gtk::DialogError::Dismissed)
                        || error.matches(gtk::DialogError::Cancelled) =>
                {
                    this.cancel_item(index);
                }
                Err(error) => {
                    this.fail(
                        index,
                        format!("Could not choose a download destination: {error}"),
                    );
                    download.cancel();
                }
            }
        });
    }

    fn waiting_for_destination(&self, index: usize, download: &webkit::Download) -> bool {
        self.items.borrow().get(index).is_some_and(|item| {
            item.status == "Waiting for location" && item.download.as_ref() == Some(download)
        })
    }

    fn set_destination(&self, index: usize, download: &webkit::Download, path: PathBuf) {
        if !self.waiting_for_destination(index, download) {
            return;
        }
        if let Err(error) = ensure_destination_is_fresh(&path) {
            self.fail(index, error);
            download.cancel();
            return;
        }
        let Some(destination) = path
            .to_str()
            .filter(|_| path.is_absolute())
            .map(str::to_owned)
        else {
            self.fail(
                index,
                "Destination must be an absolute UTF-8 filesystem path.".into(),
            );
            download.cancel();
            return;
        };
        download.set_allow_overwrite(false);
        {
            let mut items = self.items.borrow_mut();
            let item = &mut items[index];
            item.path = Some(path);
            item.status = "Downloading".into();
            item.label.set_label(&format!(
                "{} — Downloading",
                item.path
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ));
        }
        // WebKitGTK 6.0 takes an absolute path, not the legacy file:// URI.
        // State is updated first because this can emit terminal signals.
        download.set_destination(&destination);
    }

    fn update_progress(&self, index: usize, download: &webkit::Download) {
        let progress = download.estimated_progress().clamp(0.0, 1.0);
        let received = download.received_data_length();
        let total = download.response().map(|r| r.content_length()).unwrap_or(0);
        let mut items = self.items.borrow_mut();
        let item = &mut items[index];
        item.received = received;
        item.progress.set_fraction(progress);
        item.progress.set_text(Some(&format!(
            "{} / {}",
            format_size(received),
            if total == 0 {
                "Unknown size".into()
            } else {
                format_size(total)
            }
        )));
    }

    fn finish(&self, index: usize) {
        let incomplete = {
            let items = self.items.borrow();
            let item = &items[index];
            item.download.as_ref().and_then(|download| {
                let response = download.response()?;
                let headers = response.http_headers()?;
                // Encoded responses may report wire length but deliver decoded
                // bytes. Only compare like-for-like, known identity lengths.
                if headers
                    .one("Content-Encoding")
                    .is_some_and(|encoding| !encoding.eq_ignore_ascii_case("identity"))
                {
                    return None;
                }
                let expected = response.content_length();
                let received = download.received_data_length();
                (expected > 0 && expected != received).then_some((received, expected))
            })
        };
        if let Some((received, expected)) = incomplete {
            self.fail(index, format!("Incomplete download: received {received} bytes, expected {expected}. A partial file may remain."));
            return; // The native download is already terminal; never cancel it again.
        }
        let mut items = self.items.borrow_mut();
        let item = &mut items[index];
        if item.status != "Downloading" {
            return;
        }
        item.status = "Completed".into();
        if let Some(download) = item.download.take() {
            item.received = download.received_data_length();
        }
        item.reveal.set_visible(item.path.is_some());
        item.progress.set_fraction(1.0);
        item.progress.set_text(Some("Completed"));
        item.label.set_label(&format!(
            "{} — Completed",
            item.path
                .as_ref()
                .and_then(|p| p.file_name())
                .unwrap_or_default()
                .to_string_lossy()
        ));
        item.cancel.set_sensitive(false);
        if let Some(path) = &item.path {
            self.completed.borrow_mut().push(path.clone());
        }
        drop(items);
        self.record_terminal(index, Outcome::Completed);
    }

    fn fail(&self, index: usize, message: String) {
        let mut items = self.items.borrow_mut();
        let item = &mut items[index];
        if matches!(item.status.as_str(), "Failed" | "Cancelled" | "Completed") {
            return;
        }
        self.errors.borrow_mut().push(message.clone());
        item.status = "Failed".into();
        if let Some(download) = item.download.take() {
            item.received = download.received_data_length();
        }
        item.label.set_label(&format!(
            "{} — {message}",
            item.path
                .as_ref()
                .and_then(|p| p.file_name())
                .unwrap_or_default()
                .to_string_lossy()
        ));
        item.progress.set_text(Some("Failed"));
        item.cancel.set_sensitive(false);
        item.retry.set_visible(item.source_uri.is_some());
        let chooser = item.chooser.clone();
        drop(items);
        chooser.cancel();
        self.record_terminal(index, Outcome::Failed);
    }

    pub fn shutdown(&self) {
        let count = self.items.borrow().len();
        for index in 0..count {
            self.cancel_item(index);
        }
        self.window.destroy();
    }

    fn cancel_item(&self, index: usize) {
        let (download, chooser) = {
            let mut items = self.items.borrow_mut();
            let item = &mut items[index];
            if !matches!(
                item.status.as_str(),
                "Starting" | "Waiting for location" | "Downloading"
            ) {
                return;
            }
            if let Some(download) = item.download.as_ref() {
                item.received = download.received_data_length();
            }
            item.status = "Cancelled".into();
            item.label.set_label("Download cancelled");
            item.progress.set_text(Some("Cancelled"));
            item.cancel.set_sensitive(false);
            (item.download.take(), item.chooser.clone())
        };
        chooser.cancel();
        // cancel may emit failed synchronously; release RefCell first.
        if let Some(download) = download {
            download.cancel();
        }
        self.record_terminal(index, Outcome::Cancelled);
    }

    fn retry_item(self: &Rc<Self>, index: usize) {
        let uri = self
            .items
            .borrow()
            .get(index)
            .and_then(|i| i.source_uri.clone());
        if let Some(uri) = uri {
            let view = self
                .items
                .borrow()
                .get(index)
                .and_then(|i| i.source_view.as_ref())
                .and_then(|view| view.upgrade());
            let download = view.and_then(|view| view.download_uri(&uri));
            // The session's download-started/decide-destination path owns the
            // replacement item. Starting it here would create two choosers.
            if download.is_none() {
                self.errors
                    .borrow_mut()
                    .push("Could not restart this download.".into());
            }
        }
    }

    fn weak_self(&self) -> std::rc::Weak<Self> {
        self.self_weak.borrow().clone()
    }
}

fn checked_reveal_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("The saved destination is not an absolute path.".into());
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("The saved file is unavailable: {e}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(
            "The saved destination is not a regular file (symlinks are not revealed).".into(),
        );
    }
    Ok(path.to_path_buf())
}

fn safe_basename(name: &str) -> String {
    let leaf = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let cleaned: String = leaf
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\' && *c != ':')
        .take(180)
        .collect();
    let cleaned = cleaned.trim().trim_matches('.');
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "download".into()
    } else {
        cleaned.into()
    }
}

fn fresh_file_path(directory: &Path, name: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(directory)
        .map_err(|e| format!("Download directory is unavailable: {e}"))?;
    if !root.is_dir() {
        return Err("Download directory is not a directory.".into());
    }
    for suffix in 0u32..10000 {
        let dirname = unique_name(&format!("{}.download", name), suffix);
        let folder = root.join(dirname);
        match fs::create_dir(&folder) {
            Ok(()) => return Ok(folder.join(name)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Could not create a private download directory: {error}"
                ));
            }
        }
    }
    Err("Could not find an unused download directory.".into())
}

fn canonical_destination(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "The selected destination has no parent directory.".to_owned())?;
    let name = path
        .file_name()
        .ok_or_else(|| "The selected destination has no filename.".to_owned())?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|e| format!("Could not access selected directory: {e}"))?;
    if !canonical_parent.is_dir() {
        return Err("The selected destination directory is not a directory.".into());
    }
    Ok(canonical_parent.join(name))
}

fn ensure_destination_is_fresh(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err("That destination already exists; choose a new filename.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not verify download destination: {error}")),
    }
}

fn unique_name(name: &str, suffix: u32) -> String {
    if suffix == 0 {
        return name.to_owned();
    }
    let path = Path::new(name);
    match (
        path.file_stem().and_then(|s| s.to_str()),
        path.extension().and_then(|s| s.to_str()),
    ) {
        (Some(stem), Some(extension)) => format!("{stem} ({suffix}).{extension}"),
        (Some(stem), None) => format!("{stem} ({suffix})"),
        _ => format!("download ({suffix})"),
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut amount = bytes as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit < UNITS.len() - 1 {
        amount /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{amount:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
pub(crate) struct PendingChoice {
    downloads: Rc<Downloads>,
    download: webkit::Download,
}
#[cfg(test)]
impl PendingChoice {
    pub fn new(parent: &gtk::ApplicationWindow, download: &webkit::Download) -> Self {
        let downloads = Downloads::new(parent);
        // Exercise the shared waiting state and real terminal signals without
        // opening the desktop's external file-chooser portal during tests.
        downloads.prepare(download);
        Self {
            downloads,
            download: download.clone(),
        }
    }
    pub fn cancel_row(&self) {
        self.downloads.cancel_item(0);
    }
    pub fn interrupt_transfer(&self) {
        self.download.cancel();
    }
    pub fn status(&self) -> String {
        self.downloads.items.borrow()[0].status.clone()
    }
    pub fn records(&self) -> DownloadHistory {
        self.downloads.snapshot()
    }
    pub fn accept_late_path(&self, path: PathBuf) {
        self.downloads.set_destination(0, &self.download, path);
    }
}
#[cfg(test)]
impl Drop for PendingChoice {
    fn drop(&mut self) {
        self.downloads.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_reveal_path, fresh_file_path, safe_basename, unique_name};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("browny-download-test-{id}"));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn suggested_names_cannot_escape_or_select_special_paths() {
        for (input, expected) in [
            ("../../secret.txt", "secret.txt"),
            (r"..\..\secret.txt", "secret.txt"),
            ("/", "download"),
            ("...", "download"),
        ] {
            assert_eq!(safe_basename(input), expected);
        }
        assert!(!safe_basename("../a\nsecret.txt").contains('\n'));
    }

    #[test]
    fn automated_destination_uses_create_new_and_allocates_collisions() {
        let directory = temp_dir();
        let first = fresh_file_path(&directory, "report.txt").unwrap();
        let second = fresh_file_path(&directory, "report.txt").unwrap();
        assert_eq!(first.file_name().unwrap(), "report.txt");
        assert_eq!(second.file_name().unwrap(), "report.txt");
        assert_ne!(first.parent(), second.parent());
        assert!(first.parent().unwrap().is_dir() && second.parent().unwrap().is_dir());
        assert!(!first.exists() && !second.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn reveal_requires_an_existing_absolute_regular_file_not_a_symlink() {
        let directory = temp_dir();
        let file = directory.join("fixture.txt");
        fs::write(&file, "test").unwrap();
        assert_eq!(checked_reveal_path(&file).unwrap(), file);
        assert!(checked_reveal_path(&directory).is_err());
        assert!(checked_reveal_path(std::path::Path::new("relative.txt")).is_err());
        assert!(checked_reveal_path(&directory.join("missing")).is_err());
        #[cfg(unix)]
        {
            let link = directory.join("link");
            std::os::unix::fs::symlink(&file, &link).unwrap();
            assert!(checked_reveal_path(&link).is_err());
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn duplicate_name_preserves_extension() {
        assert_eq!(unique_name("archive.tar.gz", 2), "archive.tar (2).gz");
    }
}
