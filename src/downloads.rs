use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::prelude::*;
use paisley::download_history::{DownloadHistory, DownloadRecord, Outcome};
use webkit::prelude::*;

// Diagnostic completion/error snapshots must not grow with session duration.
fn remember<T>(history: &RefCell<Vec<T>>, value: T) {
    let mut history = history.borrow_mut();
    if history.len() >= 200 {
        let remove = history.len() - 199;
        history.drain(..remove);
    }
    history.push(value);
}

struct Item {
    row: gtk::Box,
    reveal: gtk::Button,
    received: u64,
    label: gtk::Label,
    detail: gtk::Label,
    icon: gtk::Image,
    size_cell: gtk::Label,
    status_cell: gtk::Label,
    date_cell: gtk::Label,
    progress: gtk::ProgressBar,
    cancel: gtk::Button,
    retry: gtk::Button,
    download: Option<webkit::Download>,
    source_uri: Option<String>,
    source_view: Option<gtk::glib::WeakRef<webkit::WebView>>,
    path: Option<PathBuf>,
    partial: Option<PathBuf>,
    status: String,
    chooser: gtk::gio::Cancellable,
}

pub struct Downloads {
    managed: RefCell<Option<Rc<crate::managed_ui::Manager>>>,
    #[cfg(feature = "cef")]
    native: RefCell<Option<Rc<crate::cef_downloads::Manager>>>,
    managed_box: gtk::Box,
    controls: gtk::Box,
    parent: gtk::ApplicationWindow,
    window: gtk::Window,
    dock: gtk::Box,
    sheet: gtk::Box,
    close: gtk::Button,
    empty: gtk::Box,
    current_heading: gtk::Label,
    saved_heading: gtk::Label,
    summary: gtk::Label,
    previous_focus: RefCell<Option<gtk::glib::WeakRef<gtk::Widget>>>,
    list: gtk::Box,
    saved_list: gtk::Box,
    message: gtk::Label,
    storage_warning: gtk::Label,
    storage_enabled: std::cell::Cell<bool>,
    history: RefCell<DownloadHistory>,
    previous: RefCell<Vec<DownloadRecord>>,
    changed: RefCell<Option<Box<dyn Fn()>>>,
    event: RefCell<Option<Box<dyn Fn(&str, &str)>>>,
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
        profile: &paisley::profile::Profile,
    ) -> Rc<Self> {
        let this = Self::build(parent, None, history, profile.private);
        if !profile.private {
            let managed = crate::managed_ui::Manager::new(parent, &profile.data_dir);
            this.controls.prepend(&managed.button);
            this.managed_box.append(&managed.widget);
            let weak = Rc::downgrade(&this);
            managed.on_changed(move || {
                if let Some(this) = weak.upgrade() {
                    this.refresh_summary();
                }
            });
            let weak = Rc::downgrade(&this);
            managed.on_event(move |title, body| {
                if let Some(this) = weak.upgrade()
                    && let Some(callback) = this.event.borrow().as_ref()
                {
                    callback(title, body);
                }
            });
            *this.managed.borrow_mut() = Some(managed);
            this.refresh_summary();
        }
        this
    }

    #[cfg(feature = "cef")]
    pub fn bind_native(self: &Rc<Self>, session: Rc<cef_gtk::ProfileSession>) {
        let existing = self.native.borrow().clone();
        let manager = existing.unwrap_or_else(|| {
            let manager = crate::cef_downloads::Manager::new(&self.parent);
            self.list.prepend(&manager.widget);
            let weak = Rc::downgrade(self);
            manager.on_changed(move || {
                if let Some(this) = weak.upgrade() {
                    this.refresh_summary();
                }
            });
            let weak = Rc::downgrade(self);
            manager.on_record(move |record| {
                if let Some(this) = weak.upgrade() {
                    if this.history.borrow_mut().record(
                        Some(&record.source),
                        record.destination.as_deref(),
                        record.outcome,
                        record.received,
                        now(),
                    ) {
                        this.history_changed();
                    }
                    if record.outcome == Outcome::Completed {
                        if let Some(path) = record.destination {
                            remember(&this.completed, path);
                        }
                    }
                    if let Some(event) = this.event.borrow().as_ref() {
                        if record.outcome != Outcome::Cancelled {
                            event(
                                if record.outcome == Outcome::Completed {
                                    "Download complete"
                                } else {
                                    "Download interrupted"
                                },
                                "See Downloads for details",
                            );
                        }
                    }
                }
            });
            self.native.replace(Some(manager.clone()));
            manager
        });
        manager.bind(session);
    }
    #[cfg(feature = "cef")]
    pub fn native_disconnected(&self) {
        if let Some(manager) = self.native.borrow().clone() {
            manager.disconnected();
        }
    }
    #[cfg(feature = "cef")]
    pub fn native_event(self: &Rc<Self>, event: cef_gtk::proto_ipc::servo_event::Event) {
        let Some(manager) = self.native.borrow().clone() else {
            return;
        };
        match event {
            cef_gtk::proto_ipc::servo_event::Event::DownloadStarted(info) => {
                manager.start(info);
                self.show();
            }
            cef_gtk::proto_ipc::servo_event::Event::DownloadProgress(info) => {
                manager.progress(info)
            }
            _ => {}
        }
    }
    #[cfg(all(test, feature = "cef"))]
    pub fn native_test_manager(&self) -> Rc<crate::cef_downloads::Manager> {
        self.native.borrow().clone().unwrap()
    }

    pub fn on_event(&self, event: impl Fn(&str, &str) + 'static) {
        *self.event.borrow_mut() = Some(Box::new(event));
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
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let saved_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let current_heading = crate::download_ui::label("This session", "downloads-section");
        let saved_heading = crate::download_ui::label("Earlier downloads", "downloads-section");
        let managed_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        rows.append(&managed_box);
        rows.append(&current_heading);
        rows.append(&list);
        rows.append(&saved_heading);
        rows.append(&saved_list);
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
        empty.add_css_class("downloads-empty");
        empty.set_valign(gtk::Align::Center);
        let empty_icon = gtk::Image::from_icon_name("lucide-download-symbolic");
        empty_icon.set_pixel_size(40);
        empty.append(&empty_icon);
        empty.append(&gtk::Label::new(Some("No downloads yet")));
        empty.append(&gtk::Label::new(Some(
            "Files you download will appear here.",
        )));
        rows.append(&empty);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&rows)
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.add_css_class("downloads-sheet");
        content.set_widget_name("paisley-downloads-sheet");
        content.set_halign(gtk::Align::Fill);
        content.set_valign(gtk::Align::Fill);
        content.set_width_request(400);
        content.set_hexpand(false);
        content.set_vexpand(true);
        content.set_visible(false);
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 4);
        titles.set_hexpand(true);
        titles.append(&crate::download_ui::label("Downloads", "downloads-heading"));
        let summary = crate::download_ui::label("Your downloaded files", "downloads-detail");
        titles.append(&summary);
        heading.append(&titles);
        let clear = gtk::Button::with_label("Clear history…");
        clear.add_css_class("flat");
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.append(&clear);

        let close = gtk::Button::from_icon_name("lucide-x-symbolic");
        close.set_tooltip_text(Some("Close downloads (Esc)"));
        close.add_css_class("flat");
        heading.append(&close);
        content.append(&heading);
        content.append(&controls);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let storage_warning = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .visible(false)
            .build();
        storage_warning.add_css_class("error");
        content.append(&storage_warning);
        let message = gtk::Label::builder().wrap(true).xalign(0.0).build();
        message.add_css_class("error");
        content.append(&message);

        content.append(&scroller);
        let notice = if private {
            "Private session · Records disappear when you close Paisley. Saved files stay on disk."
        } else {
            "Clearing history keeps your downloaded files."
        };
        let notice = gtk::Label::builder()
            .label(notice)
            .wrap(true)
            .xalign(0.0)
            .build();
        notice.add_css_class("downloads-detail");
        notice.set_tooltip_text(Some("Downloads never open automatically."));
        content.append(&notice);
        let dock = match parent.child() {
            Some(child) if child.widget_name() == "paisley-downloads-layout" => {
                child.downcast::<gtk::Box>().expect("downloads layout")
            }
            child => {
                parent.set_child(gtk::Widget::NONE);
                let dock = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                dock.set_widget_name("paisley-downloads-layout");
                if let Some(child) = child {
                    child.set_hexpand(true);
                    child.set_vexpand(true);
                    dock.append(&child);
                }
                parent.set_child(Some(&dock));
                dock
            }
        };
        dock.append(&content);
        let window: gtk::Window = parent.clone().upcast();
        let this = Rc::new_cyclic(|weak| Self {
            managed: RefCell::new(None),
            #[cfg(feature = "cef")]
            native: RefCell::new(None),
            managed_box,
            controls,
            parent: parent.clone(),
            window,
            dock,
            sheet: content,
            close,
            empty,
            current_heading,
            saved_heading,
            summary,
            previous_focus: RefCell::new(None),
            list,
            saved_list,
            message,
            storage_warning,
            storage_enabled: std::cell::Cell::new(true),
            previous: RefCell::new(history.records.clone()),
            history: RefCell::new(history),
            changed: RefCell::new(None),
            event: RefCell::new(None),
            items: RefCell::new(Vec::new()),
            completed: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
            test_directory,
            self_weak: RefCell::new(weak.clone()),
        });
        let weak = Rc::downgrade(&this);
        this.close.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.hide();
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&this);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape
                && let Some(this) = weak.upgrade()
                && this.sheet.is_visible()
            {
                this.hide();
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        // Owned by the sheet, so replacing a controller cannot accumulate handlers.
        this.sheet.add_controller(keys);
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
                    if let Some(manager)=this.managed.borrow().as_ref(){manager.clear_completed();}
                    for item in this.items.borrow().iter() {
                        if matches!(item.status.as_str(), "Completed" | "Failed" | "Cancelled") { item.row.set_visible(false); }
                    }
                    #[cfg(feature="cef")]
                    if let Some(native)=this.native.borrow().as_ref(){native.clear_completed();}
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
        self.storage_warning.set_text(&format!("Saved download records are unavailable: {reason}. The existing file is preserved. New records and clearing are memory-only until the file is explicitly repaired and Paisley is restarted."));
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
            let filename = record
                .destination
                .as_deref()
                .and_then(|path| Path::new(path).file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Download".into());
            let row = gtk::Box::new(gtk::Orientation::Vertical, 8);
            row.add_css_class("downloads-row");
            let file = gtk::Box::new(gtk::Orientation::Horizontal, 14);
            file.set_hexpand(true);
            file.set_width_request(160);
            file.append(&crate::download_ui::icon(&filename));
            let text = gtk::Box::new(gtk::Orientation::Vertical, 5);
            text.set_hexpand(true);
            let title = crate::download_ui::label(&filename, "downloads-filename");
            title.set_tooltip_text(Some(&filename));
            text.append(&title);
            let path = record
                .destination
                .as_deref()
                .unwrap_or("No destination selected");

            let source = crate::download_ui::label(
                &crate::download_ui::source(record.source.as_deref()),
                "downloads-detail",
            );
            source.set_tooltip_text(record.source.as_deref());
            text.append(&source);
            file.append(&text);
            row.append(&file);
            let metadata = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            metadata.append(&crate::download_ui::cell(&format_size(record.received), 80));
            metadata.append(&crate::download_ui::cell(
                &format!("{:?}", record.outcome),
                100,
            ));
            row.append(&metadata);
            title.set_tooltip_text(Some(&format!(
                "{path}\n{}",
                crate::download_ui::date(record.finished)
            )));
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            actions.set_width_request(180);
            actions.set_valign(gtk::Align::Center);
            if record.outcome == Outcome::Completed
                && let Some(path) = &record.destination
            {
                let reveal = gtk::Button::with_label("Show in folder");
                reveal.set_valign(gtk::Align::Center);
                let path = PathBuf::from(path);
                let weak = self.weak_self();
                reveal.connect_clicked(move |_| {
                    if let Some(this) = weak.upgrade() {
                        this.reveal(&path);
                    }
                });
                actions.append(&reveal);
            }
            row.append(&actions);
            self.saved_list.append(&row);
        }
        self.refresh_summary();
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
        if outcome != Outcome::Cancelled
            && let Some(callback) = self.event.borrow().as_ref()
        {
            let name = self.items.borrow()[index]
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "File".into());
            callback(
                if outcome == Outcome::Completed {
                    "Download complete"
                } else {
                    "Download interrupted"
                },
                &name,
            );
        }
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
        self.refresh_summary();
    }

    pub fn show(&self) {
        if !self.sheet.is_visible() {
            *self.previous_focus.borrow_mut() =
                gtk::prelude::GtkWindowExt::focus(&self.parent).map(|widget| widget.downgrade());
        }
        self.sheet.set_visible(true);
        self.close.grab_focus();
    }

    pub fn is_open(&self) -> bool {
        self.sheet.is_visible()
    }

    pub fn toggle(&self) {
        if !self.dismiss() {
            self.show();
        }
    }

    pub fn dismiss(&self) -> bool {
        if !self.sheet.is_visible() {
            return false;
        }
        self.hide();
        true
    }

    fn hide(&self) {
        if !self.sheet.is_visible() {
            return;
        }
        self.sheet.set_visible(false);
        if let Some(focus) = self
            .previous_focus
            .borrow_mut()
            .take()
            .and_then(|weak| weak.upgrade())
        {
            focus.grab_focus();
        }
    }

    fn refresh_summary(&self) {
        let items = self.items.borrow();
        let visible = items.iter().filter(|item| item.row.is_visible()).count();
        let active = items
            .iter()
            .filter(|item| {
                matches!(
                    item.status.as_str(),
                    "Starting" | "Waiting for location" | "Downloading"
                )
            })
            .count();
        let saved = self.previous.borrow().len();
        #[cfg(feature = "cef")]
        let (native, native_active) = self.native.borrow().as_ref().map_or((0, 0), |m| m.counts());
        #[cfg(not(feature = "cef"))]
        let (native, native_active) = (0, 0);
        let (managed, managed_active) = self
            .managed
            .borrow()
            .as_ref()
            .map_or((0, 0), |manager| manager.counts());
        self.current_heading.set_visible(visible + native > 0);
        self.saved_heading.set_visible(saved > 0);
        self.empty
            .set_visible(visible + native + saved + managed == 0);
        self.summary.set_text(&format!(
            "{} active · {} {}",
            active + managed_active + native_active,
            visible + native + saved + managed,
            if visible + native + saved + managed == 1 {
                "file"
            } else {
                "files"
            }
        ));
    }

    #[cfg(test)]
    pub fn completed_paths(&self) -> Vec<PathBuf> {
        self.completed.borrow().clone()
    }
    #[cfg(test)]
    pub fn errors(&self) -> Vec<String> {
        self.errors.borrow().clone()
    }
    #[cfg(any(test, feature = "cef"))]
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
            + self
                .managed
                .borrow()
                .as_ref()
                .map_or(0, |manager| manager.counts().1)
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
        let row = gtk::Box::new(gtk::Orientation::Vertical, 8);
        row.add_css_class("downloads-row");
        let icon = crate::download_ui::icon(&filename);
        let file = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        file.set_hexpand(true);
        file.set_width_request(160);
        file.append(&icon);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 6);
        text.set_hexpand(true);
        let label = crate::download_ui::label(&filename, "downloads-filename");
        label.set_tooltip_text(Some(&filename));
        let detail = crate::download_ui::label(
            &format!(
                "{status} · {}",
                crate::download_ui::source(source_uri.as_deref())
            ),
            "downloads-detail",
        );
        detail.set_tooltip_text(source_uri.as_deref());
        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        text.append(&label);
        text.append(&detail);
        text.append(&progress);
        file.append(&text);
        row.append(&file);
        let size_cell = crate::download_ui::cell("—", 80);
        let status_cell = crate::download_ui::cell(status, 100);
        let date_cell = crate::download_ui::cell("—", 175);
        let metadata = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        metadata.append(&size_cell);
        metadata.append(&status_cell);
        row.append(&metadata);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions.set_valign(gtk::Align::Center);
        actions.set_width_request(180);
        let reveal = gtk::Button::with_label("Show in folder");
        reveal.set_visible(false);
        let cancel = gtk::Button::with_label("Cancel");
        let retry = gtk::Button::with_label("Retry");
        retry.set_visible(false);
        actions.append(&reveal);
        actions.append(&retry);
        actions.append(&cancel);
        row.append(&actions);
        self.list.prepend(&row);
        let index = self.items.borrow().len();
        self.items.borrow_mut().push(Item {
            row,
            reveal,
            received: 0,
            label,
            detail,
            icon,
            size_cell,
            status_cell,
            date_cell,
            progress,
            cancel,
            retry,
            download: Some(download.clone()),
            source_uri,
            source_view: download.web_view().map(|view| view.downgrade()),
            path: None,
            partial: None,
            status: status.into(),
            chooser: gtk::gio::Cancellable::new(),
        });

        self.refresh_summary();
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
                    item.status_cell.set_text("Choose location");
                    item.label.set_text(&filename);
                    item.label.set_tooltip_text(Some(&filename));
                    item.icon
                        .set_icon_name(Some(crate::download_ui::icon_name(&filename)));
                    item.detail.set_text("Waiting for location");
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
        let partial = match native_partial(&path) {
            Ok(partial) => partial,
            Err(error) => {
                self.fail(index, error);
                download.cancel();
                return;
            }
        };
        download.set_allow_overwrite(false);
        {
            let mut items = self.items.borrow_mut();
            let item = &mut items[index];
            item.path = Some(path);
            item.partial = Some(partial.clone());
            item.status = "Downloading".into();
            item.status_cell.set_text("Downloading");
            let filename = item
                .path
                .as_ref()
                .unwrap()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            item.label.set_text(&filename);
            item.label.set_tooltip_text(Some(&filename));
            item.icon
                .set_icon_name(Some(crate::download_ui::icon_name(&filename)));
            item.detail.set_text("Downloading");
            item.detail.set_tooltip_text(Some(&destination));
        }
        // WebKitGTK 6.0 takes an absolute path, not the legacy file:// URI.
        // State is updated first because this can emit terminal signals.
        download.set_destination(partial.to_str().expect("UTF-8 selected destination"));
    }

    fn update_progress(&self, index: usize, download: &webkit::Download) {
        let progress = download.estimated_progress().clamp(0.0, 1.0);
        let received = download.received_data_length();
        let total = download.response().map(|r| r.content_length()).unwrap_or(0);
        let mut items = self.items.borrow_mut();
        let item = &mut items[index];
        item.received = received;
        item.size_cell.set_text(&format_size(received));
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
        let publication = {
            let items = self.items.borrow();
            let item = &items[index];
            if item.status != "Downloading" {
                return;
            }
            match (&item.partial, &item.path) {
                (Some(partial), Some(destination)) => publish_native(partial, destination),
                _ => Err("Missing native partial file; final destination was not published".into()),
            }
        };
        if let Err(error) = publication {
            self.fail(index, error);
            return;
        }
        let mut items = self.items.borrow_mut();
        let item = &mut items[index];
        item.status = "Completed".into();
        item.status_cell.set_text("Completed");
        item.date_cell.set_text(&crate::download_ui::date(now()));
        if let Some(download) = item.download.take() {
            item.received = download.received_data_length();
        }
        item.size_cell.set_text(&format_size(item.received));
        item.reveal.set_visible(item.path.is_some());
        item.progress.set_fraction(1.0);
        item.progress.set_text(Some("Completed"));
        item.detail
            .set_text(&format!("Completed · {}", format_size(item.received)));
        item.progress.set_visible(false);
        item.cancel.set_visible(false);
        if let Some(path) = &item.path {
            remember(&self.completed, path.clone());
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
        remember(&self.errors, message.clone());
        item.status = "Failed".into();
        item.status_cell.set_text("Failed");
        item.date_cell.set_text(&crate::download_ui::date(now()));
        if let Some(download) = item.download.take() {
            item.received = download.received_data_length();
        }
        item.detail.set_text(&message);
        item.detail.set_tooltip_text(Some(&message));
        item.detail.set_ellipsize(gtk::pango::EllipsizeMode::None);
        item.detail.set_wrap(true);
        item.detail.add_css_class("error");
        item.size_cell.set_text(&format_size(item.received));
        item.progress.set_visible(true);
        item.cancel.set_visible(false);
        item.retry.set_visible(item.source_uri.is_some());
        let chooser = item.chooser.clone();
        drop(items);
        chooser.cancel();
        self.record_terminal(index, Outcome::Failed);
    }

    pub fn shutdown(&self) {
        #[cfg(feature = "cef")]
        if let Some(native) = self.native.borrow().as_ref() {
            native.shutdown();
        }
        if let Some(managed) = self.managed.borrow().as_ref() {
            managed.shutdown();
        }
        let count = self.items.borrow().len();
        for index in 0..count {
            self.cancel_item(index);
        }
        self.hide();
        if self.sheet.parent().is_some() {
            self.dock.remove(&self.sheet);
        }
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
            item.status_cell.set_text("Cancelled");
            item.date_cell.set_text(&crate::download_ui::date(now()));
            item.detail.set_text(&format!(
                "Cancelled · {} received",
                format_size(item.received)
            ));
            item.progress.set_text(Some("Cancelled"));
            item.cancel.set_visible(false);
            item.progress.set_visible(false);
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
                remember(&self.errors, "Could not restart this download.".into());
            }
        }
    }

    fn weak_self(&self) -> std::rc::Weak<Self> {
        self.self_weak.borrow().clone()
    }
}

impl Drop for Downloads {
    fn drop(&mut self) {
        self.hide();
        if self.sheet.parent().is_some() {
            self.dock.remove(&self.sheet);
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
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

pub(crate) fn safe_basename(name: &str) -> String {
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

pub(crate) fn fresh_file_path(directory: &Path, name: &str) -> Result<PathBuf, String> {
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

pub(crate) fn canonical_destination(path: &Path) -> Result<PathBuf, String> {
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

pub(crate) fn ensure_destination_is_fresh(path: &Path) -> Result<(), String> {
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

pub(crate) fn format_size(bytes: u64) -> String {
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
        let path = std::env::temp_dir().join(format!("paisley-download-test-{id}"));
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
    fn diagnostic_history_retains_only_the_latest_200_entries() {
        let history = std::cell::RefCell::new(Vec::new());
        for value in 0..10000 {super::remember(&history, value);}
        let history = history.borrow();
        assert_eq!(history.len(), 200);
        assert_eq!(history.first(), Some(&9800));
        assert_eq!(history.last(), Some(&9999));
    }

    #[test]
    fn discarded_native_download_removes_only_its_staging_files() {
        let directory = temp_dir();
        let destination = directory.join("report.txt");
        fs::write(&destination, "existing download").unwrap();
        let partial = super::native_partial(&destination).unwrap();
        fs::write(&partial, "unfinished bytes").unwrap();
        super::discard_native(&partial);
        assert!(!partial.exists());
        assert!(!partial.parent().unwrap().exists());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "existing download");
        super::discard_native(&partial); // Repeated terminal events are harmless.
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn duplicate_name_preserves_extension() {
        assert_eq!(unique_name("archive.tar.gz", 2), "archive.tar (2).gz");
    }
}

pub(crate) fn native_partial(destination: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    let parent = destination.parent().ok_or("Missing download parent")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = parent.join(format!(
        ".paisley-native-{}-{stamp:x}.part",
        std::process::id()
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|e| format!("Cannot stage download: {e}"))?;
    Ok(directory.join("payload.part"))
}
pub(crate) fn publish_native(partial: &Path, destination: &Path) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(partial)
        .map_err(|e| format!("Cannot read completed partial: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Download partial is not a regular file".into());
    }
    file.sync_all()
        .map_err(|e| format!("Cannot sync completed download: {e}"))?;
    std::fs::hard_link(partial, destination)
        .map_err(|e| format!("Cannot publish download without overwriting: {e}"))?;
    std::fs::File::open(destination.parent().ok_or("Missing download directory")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| format!("File was linked but directory sync failed: {e}"))?;
    let _ = std::fs::remove_file(partial);
    if let Some(directory) = partial.parent() {
        let _ = std::fs::remove_dir(directory);
    }
    Ok(())
}

/// Remove only the private leaf and empty directory allocated by native_partial.
/// Never recursively delete a download directory or the selected destination.
pub(crate) fn discard_native(partial: &Path) {
    let _ = std::fs::remove_file(partial);
    if let Some(directory) = partial.parent() {
        let _ = std::fs::remove_dir(directory);
    }
}
