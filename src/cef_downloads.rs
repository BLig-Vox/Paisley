//! Native CEF transfers shown inside the existing download sheet.
use cef_gtk::{
    ProfileSession,
    proto_ipc::{
        DownloadControl, DownloadDestination, NativeDownload, StartDownload, servo_action::Action,
    },
};
use gtk::prelude::*;
use paisley::download_history::Outcome;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
};
type Key = (u64, u32);
pub struct Record {
    pub source: String,
    pub destination: Option<PathBuf>,
    pub outcome: Outcome,
    pub received: u64,
}
struct Row {
    widget: gtk::Box,
    label: gtk::Label,
    detail: gtk::Label,
    progress: gtk::ProgressBar,
    pause: gtk::Button,
    cancel: gtk::Button,
    retry: gtk::Button,
    reveal: gtk::Button,
    source: String,
    method: String,
    destination: Option<PathBuf>,
    partial: Option<PathBuf>,
    received: u64,
    total: i64,
    terminal: bool,
    paused: bool,
    session: Option<Rc<ProfileSession>>,
    chooser: gtk::gio::Cancellable,
}
pub struct Manager {
    pub widget: gtk::Box,
    parent: gtk::glib::WeakRef<gtk::ApplicationWindow>,
    rows: RefCell<BTreeMap<Key, Row>>,
    epoch: Cell<u64>,
    session: RefCell<Option<Rc<ProfileSession>>>,
    changed: RefCell<Option<Box<dyn Fn()>>>,
    record: RefCell<Option<Box<dyn Fn(Record)>>>,
    test_directory: RefCell<Option<PathBuf>>,
}
impl Manager {
    pub fn new(parent: &gtk::ApplicationWindow) -> Rc<Self> {
        Rc::new(Self {
            widget: gtk::Box::new(gtk::Orientation::Vertical, 0),
            parent: parent.downgrade(),
            rows: RefCell::new(BTreeMap::new()),
            epoch: Cell::new(0),
            session: RefCell::new(None),
            changed: RefCell::new(None),
            record: RefCell::new(None),
            test_directory: RefCell::new(None),
        })
    }
    pub fn bind(&self, session: Rc<ProfileSession>) {
        self.shutdown();
        self.epoch.set(self.epoch.get() + 1);
        self.session.replace(Some(session));
    }
    pub fn on_changed(&self, f: impl Fn() + 'static) {
        self.changed.replace(Some(Box::new(f)));
    }
    pub fn on_record(&self, f: impl Fn(Record) + 'static) {
        self.record.replace(Some(Box::new(f)));
    }
    fn changed(&self) {
        if let Some(f) = self.changed.borrow().as_ref() {
            f();
        }
    }
    pub fn counts(&self) -> (usize, usize) {
        let rows = self.rows.borrow();
        (
            rows.values().filter(|r| r.widget.is_visible()).count(),
            rows.values().filter(|r| !r.terminal).count(),
        )
    }
    #[cfg(test)]
    pub fn set_directory(&self, path: PathBuf) {
        self.test_directory.replace(Some(path));
    }
    #[cfg(test)]
    pub fn test_control(&self, id: u32, operation: &str) {
        self.control((self.epoch.get(), id), operation);
    }
    #[cfg(test)]
    pub fn snapshots(&self) -> Vec<(u32, u64, bool, bool)> {
        self.rows
            .borrow()
            .iter()
            .map(|((_, id), r)| (*id, r.received, r.paused, r.terminal))
            .collect()
    }
    #[cfg(test)]
    pub fn completed_paths(&self) -> Vec<PathBuf> {
        self.rows
            .borrow()
            .values()
            .filter(|r| r.terminal && r.reveal.is_visible())
            .filter_map(|r| r.destination.clone())
            .collect()
    }
    pub fn start(self: &Rc<Self>, info: NativeDownload) {
        let Some(session) = self.session.borrow().clone() else {
            return;
        };
        let key = (self.epoch.get(), info.id);
        if self.rows.borrow().contains_key(&key) {
            return;
        }
        let name = crate::downloads::safe_basename(&info.name);
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
        widget.add_css_class("downloads-row");
        let file = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        file.append(&crate::download_ui::icon(&name));
        let text = gtk::Box::new(gtk::Orientation::Vertical, 6);
        text.set_hexpand(true);
        let label = crate::download_ui::label(&name, "downloads-filename");
        label.set_tooltip_text(Some(&name));
        let detail = crate::download_ui::label("Choose a save location", "downloads-detail");
        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        progress.set_text(Some("Waiting for location"));
        text.append(&label);
        text.append(&detail);
        text.append(&progress);
        file.append(&text);
        widget.append(&file);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let pause = gtk::Button::with_label("Pause");
        pause.set_visible(false);
        let cancel = gtk::Button::with_label("Cancel");
        let retry = gtk::Button::with_label("Retry");
        retry.set_visible(false);
        let reveal = gtk::Button::with_label("Show in folder");
        reveal.set_visible(false);
        for button in [&pause, &cancel, &retry, &reveal] {
            actions.append(button);
        }
        widget.append(&actions);
        self.widget.prepend(&widget);
        self.rows.borrow_mut().insert(
            key,
            Row {
                widget,
                label,
                detail,
                progress,
                pause: pause.clone(),
                cancel: cancel.clone(),
                retry: retry.clone(),
                reveal: reveal.clone(),
                source: info.url,
                method: info.method,
                destination: None,
                partial: None,
                received: 0,
                total: info.total,
                terminal: false,
                paused: false,
                session: Some(session),
                chooser: gtk::gio::Cancellable::new(),
            },
        );
        let weak = Rc::downgrade(self);
        cancel.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.control(key, "cancel");
            }
        });
        let weak = Rc::downgrade(self);
        pause.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let paused = this.rows.borrow().get(&key).is_some_and(|r| r.paused);
                this.control(key, if paused { "resume" } else { "pause" });
            }
        });
        let weak = Rc::downgrade(self);
        retry.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let url = this.rows.borrow().get(&key).map(|r| r.source.clone());
                if let (Some(url), Some(session)) = (url, this.session.borrow().as_ref()) {
                    session.send(Action::StartDownload(StartDownload { page_id: 0, url }));
                }
            }
        });
        let weak = Rc::downgrade(self);
        reveal.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                if let Some(path) = this
                    .rows
                    .borrow()
                    .get(&key)
                    .and_then(|r| r.destination.clone())
                {
                    if let (Some(parent), Some(directory)) = (this.parent.upgrade(), path.parent())
                    {
                        gtk::FileLauncher::new(Some(&gtk::gio::File::for_path(directory))).launch(
                            Some(&parent),
                            gtk::gio::Cancellable::NONE,
                            |_| {},
                        );
                    }
                }
            }
        });
        self.changed();
        self.choose(key, &name);
        self.prune();
    }
    fn choose(self: &Rc<Self>, key: Key, name: &str) {
        if let Some(directory) = self.test_directory.borrow().clone() {
            match crate::downloads::fresh_file_path(&directory, name) {
                Ok(path) => self.destination(key, path),
                Err(error) => self.finish(key, Outcome::Failed, &error),
            }
            return;
        }
        let Some(parent) = self.parent.upgrade() else {
            self.control(key, "cancel");
            return;
        };
        let Some(cancel) = self.rows.borrow().get(&key).map(|r| r.chooser.clone()) else {
            return;
        };
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Save download");
        dialog.set_initial_name(Some(name));
        let weak = Rc::downgrade(self);
        dialog.save(Some(&parent), Some(&cancel), move |result| {
            if let Some(this) = weak.upgrade() {
                match result {
                    Ok(file) => match file.path() {
                        Some(path) => this.destination(key, path),
                        None => this.finish(key, Outcome::Failed, "Choose a local folder"),
                    },
                    Err(_) => this.control(key, "cancel"),
                }
            }
        });
    }
    fn destination(&self, key: Key, path: PathBuf) {
        if !self
            .rows
            .borrow()
            .get(&key)
            .is_some_and(|r| !r.terminal && r.destination.is_none())
        {
            return;
        }
        let selected = crate::downloads::canonical_destination(&path).and_then(|p| {
            crate::downloads::ensure_destination_is_fresh(&p)?;
            let partial = crate::downloads::native_partial(&p)?;
            Ok((p, partial))
        });
        match selected {
            Ok((path, partial)) => {
                let session = {
                    let mut rows = self.rows.borrow_mut();
                    let row = rows.get_mut(&key).unwrap();
                    row.destination = Some(path);
                    row.partial = Some(partial.clone());
                    row.detail.set_text("Starting download");
                    row.progress.set_text(Some("Starting"));
                    row.session.clone()
                };
                if let Some(session) = session {
                    session.send(Action::DownloadDestination(DownloadDestination {
                        id: key.1,
                        path: partial.to_string_lossy().into_owned(),
                    }));
                }
            }
            Err(error) => self.finish(key, Outcome::Failed, &error),
        }
    }
    pub fn progress(&self, info: NativeDownload) {
        let key = (self.epoch.get(), info.id);
        {
            let mut rows = self.rows.borrow_mut();
            let Some(row) = rows.get_mut(&key).filter(|r| !r.terminal) else {
                return;
            };
            row.received = info.received;
            row.total = info.total;
            row.paused = info.paused;
            let bytes = crate::downloads::format_size(info.received);
            let total = if info.total > 0 {
                crate::downloads::format_size(info.total as u64)
            } else {
                "Unknown size".into()
            };
            row.progress.set_fraction(if info.total > 0 {
                (info.received as f64 / info.total as f64).clamp(0., 1.)
            } else {
                0.
            });
            row.progress.set_text(Some(&format!("{bytes} / {total}")));
            if row.destination.is_some() {
                row.pause
                    .set_visible(!info.complete && !info.cancelled && !info.interrupted);
                row.pause
                    .set_label(if info.paused { "Resume" } else { "Pause" });
                row.detail.set_text(&format!(
                    "{} · {bytes} / {total}{}",
                    if info.paused { "Paused" } else { "Downloading" },
                    if info.paused || info.speed == 0 {
                        String::new()
                    } else {
                        format!(" · {}/s", crate::downloads::format_size(info.speed))
                    }
                ));
            }
        }
        if info.complete {
            let paths = self
                .rows
                .borrow()
                .get(&key)
                .and_then(|r| Some((r.partial.clone()?, r.destination.clone()?)));
            match paths.map(|(partial, path)| crate::downloads::publish_native(&partial, &path)) {
                Some(Ok(())) => self.finish(key, Outcome::Completed, "Completed"),
                Some(Err(error)) => self.finish(key, Outcome::Failed, &error),
                None => self.finish(
                    key,
                    Outcome::Failed,
                    "Download finished without a selected destination",
                ),
            }
        } else if info.cancelled {
            self.finish(key, Outcome::Cancelled, "Cancelled");
        } else if info.interrupted {
            self.finish(
                key,
                Outcome::Failed,
                "Interrupted — retry from the original page",
            );
        }
        self.changed();
    }
    pub fn control(&self, key: Key, operation: &str) {
        let state = self
            .rows
            .borrow()
            .get(&key)
            .filter(|r| !r.terminal)
            .map(|r| {
                (
                    r.session.clone(),
                    r.destination.is_none(),
                    r.chooser.clone(),
                )
            });
        if let Some((Some(session), waiting, chooser)) = state {
            if waiting && operation == "cancel" {
                chooser.cancel();
                session.send(Action::DownloadDestination(DownloadDestination {
                    id: key.1,
                    path: String::new(),
                }));
                self.finish(key, Outcome::Cancelled, "Cancelled");
            } else {
                session.send(Action::DownloadControl(DownloadControl {
                    id: key.1,
                    operation: operation.into(),
                }));
            }
        }
    }
    fn finish(&self, key: Key, outcome: Outcome, message: &str) {
        let (record, chooser, partial) = {
            let mut rows = self.rows.borrow_mut();
            let Some(row) = rows.get_mut(&key).filter(|r| !r.terminal) else {
                return;
            };
            row.terminal = true;
            let chooser = row.chooser.clone();
            let session = row.session.take();
            if outcome != Outcome::Completed {
                if let Some(session) = session {
                    session.send(Action::DownloadDestination(DownloadDestination {
                        id: key.1,
                        path: String::new(),
                    }));
                    session.send(Action::DownloadControl(DownloadControl {
                        id: key.1,
                        operation: "cancel".into(),
                    }));
                }
            }
            row.detail.set_text(&format!(
                "{message} · {} received",
                crate::downloads::format_size(row.received)
            ));
            row.pause.set_visible(false);
            row.cancel.set_visible(false);
            row.reveal
                .set_visible(outcome == Outcome::Completed && row.destination.is_some());
            row.retry.set_visible(
                outcome == Outcome::Failed
                    && row.method == "GET"
                    && url::Url::parse(&row.source)
                        .is_ok_and(|u| matches!(u.scheme(), "http" | "https")),
            );
            if outcome == Outcome::Completed {
                row.progress.set_fraction(1.);
                row.progress.set_visible(false);
            }
            (
                Record {
                    source: row.source.clone(),
                    destination: row.destination.clone(),
                    outcome,
                    received: row.received,
                },
                chooser,
                row.partial.take(),
            )
        };
        chooser.cancel();
        if let Some(partial) = partial {
            crate::downloads::discard_native(&partial);
        }
        if let Some(callback) = self.record.borrow().as_ref() {
            callback(record);
        }
        self.changed();
    }
    fn prune(&self) {
        let stale: Vec<_> = self
            .rows
            .borrow()
            .iter()
            .filter(|(_, r)| r.terminal)
            .map(|(key, _)| *key)
            .collect();
        let count = stale.len().saturating_sub(200);
        for key in stale.into_iter().take(count) {
            if let Some(row) = self.rows.borrow_mut().remove(&key) {
                self.widget.remove(&row.widget);
            }
        }
    }
    pub fn clear_completed(&self) {
        let keys: Vec<_> = self
            .rows
            .borrow()
            .iter()
            .filter(|(_, r)| r.terminal)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            if let Some(row) = self.rows.borrow_mut().remove(&key) {
                self.widget.remove(&row.widget);
            }
        }
        self.changed();
    }
    pub fn disconnected(&self) {
        let keys: Vec<_> = self
            .rows
            .borrow()
            .iter()
            .filter(|(_, r)| !r.terminal)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            self.finish(
                key,
                Outcome::Failed,
                "Browser engine stopped — retry from the original page",
            );
        }
        self.session.borrow_mut().take();
    }
    pub fn shutdown(&self) {
        let keys: Vec<_> = self
            .rows
            .borrow()
            .iter()
            .filter(|(_, r)| !r.terminal)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            self.control(key, "cancel");
            self.finish(key, Outcome::Cancelled, "Cancelled");
        }
        self.session.borrow_mut().take();
    }
}
