//! GTK ownership and queue for durable direct-URL downloads.
use gtk::prelude::*;
use paisley::managed_download::{self as engine, Job, Worker};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::Ordering,
    time::Instant,
};

struct Row {
    job: Job,
    container: gtk::Box,
    detail: gtk::Label,
    size: gtk::Label,
    status: gtk::Label,
    progress: gtk::ProgressBar,
    speed: gtk::SpinButton,
    date: gtk::Label,
    pause: gtk::Button,
    resume: gtk::Button,
    reveal: gtk::Button,
    worker: Option<Worker>,
    queued: bool,
    removed: bool,
    tick: Instant,
    bytes: u64,
    speed_sample: Option<f64>,
}
pub struct Manager {
    pub widget: gtk::Box,
    pub button: gtk::Button,
    parent: gtk::ApplicationWindow,
    rows: RefCell<Vec<Row>>,
    registry: PathBuf,
    warning: gtk::Label,
    poll: RefCell<Option<gtk::glib::SourceId>>,
    enabled: std::cell::Cell<bool>,
    changed: RefCell<Option<Box<dyn Fn()>>>,
}
impl Manager {
    pub fn new(parent: &gtk::ApplicationWindow, directory: &Path) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let warning = crate::download_ui::label("", "error");
        warning.set_wrap(true);
        warning.set_visible(false);
        widget.append(&warning);
        let button = gtk::Button::with_label("New download…");
        button.set_tooltip_text(Some(
            "Direct URL · resumable transfers, up to four connections and SHA-256 verification",
        ));
        let this = Rc::new(Self {
            widget,
            button,
            parent: parent.clone(),
            rows: RefCell::new(Vec::new()),
            registry: directory.join("managed-downloads.json"),
            warning,
            poll: RefCell::new(None),
            enabled: std::cell::Cell::new(true),
            changed: RefCell::new(None),
        });
        match engine::load(&this.registry) {
            Ok(jobs) => {
                for job in jobs {
                    this.add_row(job, false);
                }
            }
            Err(error) => {
                this.enabled.set(false);
                this.button.set_sensitive(false);
                this.warn(&format!(
                    "Managed job recovery unavailable: {error}. Existing state was preserved."
                ));
            }
        }
        let weak = Rc::downgrade(&this);
        this.button.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.prompt();
            }
        });
        let weak = Rc::downgrade(&this);
        *this.poll.borrow_mut() = Some(gtk::glib::timeout_add_local(
            std::time::Duration::from_millis(500),
            move || {
                if let Some(this) = weak.upgrade() {
                    this.poll();
                    gtk::glib::ControlFlow::Continue
                } else {
                    gtk::glib::ControlFlow::Break
                }
            },
        ));
        this
    }
    fn warn(&self, message: &str) {
        self.warning.set_text(message);
        self.warning.set_visible(true);
    }
    fn persist(&self) -> bool {
        match engine::save(
            &self.registry,
            self.rows
                .borrow()
                .iter()
                .filter(|row| !row.removed)
                .map(|row| row.job.clone())
                .collect(),
        ) {
            Ok(()) => true,
            Err(error) => {
                self.warn(&format!(
                    "Could not save managed job state: {error}. Partial files remain on disk."
                ));
                false
            }
        }
    }
    fn prompt(self: &Rc<Self>) {
        if self.rows.borrow().len() >= 100 {
            self.warn("Managed session limit reached (100). Remove old jobs and reopen Paisley to free slots.");
            return;
        }
        let dialog = gtk::Window::builder()
            .title("New download")
            .transient_for(&self.parent)
            .modal(true)
            .default_width(560)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(20);
        content.set_margin_bottom(20);
        content.set_margin_start(20);
        content.set_margin_end(20);
        content.append(&crate::download_ui::label(
            "New download",
            "downloads-heading",
        ));
        let note=gtk::Label::builder().label("Public HTTP(S) links. Browser sign-ins stay in the browser downloader. The URL (including any access token) and destination are saved in this profile.").wrap(true).xalign(0.0).build();
        let advanced_content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        advanced_content.append(&note);
        content.append(&crate::download_ui::label(
            "Download URL",
            "downloads-filename",
        ));
        let url = gtk::Entry::builder()
            .placeholder_text("https://example.com/file.zip")
            .max_length(8192)
            .build();
        content.append(&url);
        advanced_content.append(&crate::download_ui::label(
            "Checksum (SHA-256, optional)",
            "downloads-filename",
        ));
        let checksum = gtk::Entry::builder()
            .placeholder_text("64 hexadecimal characters from a trusted source")
            .max_length(64)
            .build();
        advanced_content.append(&checksum);
        let connections = gtk::SpinButton::with_range(1.0, 4.0, 1.0);
        connections.set_value(4.0);
        let limit = gtk::SpinButton::with_range(0.0, 524288.0, 16.0);
        limit.set_value(0.0);
        let options = gtk::Grid::builder()
            .row_spacing(8)
            .column_spacing(16)
            .build();
        options.attach(
            &gtk::Label::new(Some("Connections (server permitting)")),
            0,
            0,
            1,
            1,
        );
        options.attach(&connections, 1, 0, 1, 1);
        options.attach(
            &gtk::Label::new(Some("Speed limit, KiB/s (0 = unlimited)")),
            0,
            1,
            1,
            1,
        );
        options.attach(&limit, 1, 1, 1, 1);
        advanced_content.append(&options);
        let advanced = gtk::Expander::new(Some("Advanced"));
        advanced.set_child(Some(&advanced_content));
        content.append(&advanced);
        let error = gtk::Label::builder().wrap(true).xalign(0.0).build();
        error.add_css_class("error");
        content.append(&error);
        let add = gtk::Button::with_label("Save to…");
        add.add_css_class("suggested-action");
        content.append(&add);
        dialog.set_child(Some(&content));
        let weak = Rc::downgrade(self);
        let window = dialog.downgrade();
        add.connect_clicked(move |_| {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let Some(window) = window.upgrade() else {
                return;
            };
            let value = url.text().trim().to_owned();
            if let Err(message) = engine::validate_url(&value) {
                error.set_text(&message);
                return;
            }
            let sha = checksum.text().trim().to_ascii_lowercase();
            if !sha.is_empty() && (sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit())) {
                error.set_text("Use exactly 64 hexadecimal characters for SHA-256.");
                return;
            }
            let speed = limit.value() as u64 * 1024;
            if speed > 0 && speed < 16 * 1024 {
                error.set_text("Use at least 16 KiB/s, or 0 for unlimited.");
                return;
            }
            let name = url::Url::parse(&value)
                .ok()
                .and_then(|uri| {
                    uri.path_segments()
                        .and_then(|mut parts| parts.next_back())
                        .map(str::to_owned)
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "download".into());
            let chooser = gtk::FileDialog::new();
            chooser.set_title("Save download");
            chooser.set_initial_name(Some(&name));
            let weak = Rc::downgrade(&this);
            let chooser_parent = window.clone();
            let window = window.downgrade();
            let connections = connections.value() as usize;
            chooser.save(
                Some(&chooser_parent),
                None::<&gtk::gio::Cancellable>,
                move |choice| {
                    let Some(this) = weak.upgrade() else {
                        return;
                    };
                    let Ok(file) = choice else {
                        return;
                    };
                    let Some(path) = file.path() else {
                        this.warn("Choose a local destination.");
                        return;
                    };
                    let path = path
                        .parent()
                        .and_then(|parent| std::fs::canonicalize(parent).ok())
                        .zip(path.file_name())
                        .map(|(parent, name)| parent.join(name));
                    let Some(path) = path else {
                        this.warn("Destination directory is unavailable.");
                        return;
                    };
                    if path.symlink_metadata().is_ok() {
                        this.warn(
                            "Destination exists; choose another filename. It was not overwritten.",
                        );
                        return;
                    }
                    match Job::new(
                        value.clone(),
                        path,
                        (!sha.is_empty()).then_some(sha.clone()),
                        connections,
                        speed,
                    ) {
                        Ok(job) => {
                            this.add_row(job, true);
                            if !this.persist() {
                                if let Some(row) = this.rows.borrow_mut().last_mut() {
                                    row.queued = false;
                                    row.status.set_text("Not saved");
                                }
                            }
                            if let Some(window) = window.upgrade() {
                                window.close();
                            }
                        }
                        Err(error) => this.warn(&error),
                    }
                },
            );
        });
        dialog.present();
    }
    fn add_row(self: &Rc<Self>, job: Job, queued: bool) {
        let index = self.rows.borrow().len();
        let row = gtk::Box::new(gtk::Orientation::Vertical, 8);
        row.add_css_class("downloads-row");
        let file = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        file.set_hexpand(true);
        file.set_width_request(160);
        let filename = job
            .destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        file.append(&crate::download_ui::icon(&filename));
        let info = gtk::Box::new(gtk::Orientation::Vertical, 5);
        info.set_hexpand(true);
        info.append(&crate::download_ui::label(&filename, "downloads-filename"));
        let detail = crate::download_ui::label(
            if job.completed.is_some() {
                if job.sha256.is_some() {
                    "Checksum verified"
                } else {
                    "Download complete"
                }
            } else if queued {
                "Waiting to download"
            } else {
                "Ready to resume"
            },
            "downloads-detail",
        );
        detail.set_tooltip_text(Some(&job.destination.to_string_lossy()));
        info.append(&detail);
        let progress = gtk::ProgressBar::new();
        progress.set_fraction(fraction(job.received.unwrap_or(0), job.total));
        progress.set_visible(job.completed.is_none());
        info.append(&progress);
        file.append(&info);
        row.append(&file);
        let size =
            crate::download_ui::cell(&received_text(job.received.unwrap_or(0), job.total), 160);
        let status = crate::download_ui::cell(
            if job.completed.is_some() {
                if job.sha256.is_some() {
                    "Verified"
                } else {
                    "Completed"
                }
            } else if queued {
                "Pending"
            } else {
                "Paused"
            },
            100,
        );
        let metadata = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        metadata.append(&size);
        metadata.append(&status);
        row.append(&metadata);
        let date = crate::download_ui::cell(
            &job.finished
                .map_or_else(|| "—".into(), crate::download_ui::date),
            175,
        );

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions.set_valign(gtk::Align::Center);
        actions.set_width_request(180);
        let pause = gtk::Button::with_label("Pause");
        pause.set_visible(queued);
        let resume = gtk::Button::with_label("Resume");
        resume.set_visible(!queued && job.completed.is_none());
        let reveal = gtk::Button::with_label("Show in folder");
        reveal.set_visible(job.completed.is_some());
        let remove = gtk::Button::with_label("Remove from list");
        remove.set_tooltip_text(Some(
            "Removes the job record; completed and partial files stay on disk.",
        ));
        remove.add_css_class("flat");
        let limit = gtk::SpinButton::with_range(0.0, 524288.0, 16.0);
        limit.set_value((job.limit / 1024) as f64);
        limit.set_tooltip_text(Some("Per-file KiB/s; 0 is unlimited, minimum 16"));
        limit.set_visible(job.completed.is_none());
        actions.append(&pause);
        actions.append(&resume);
        actions.append(&reveal);
        let more = gtk::MenuButton::builder()
            .icon_name("lucide-ellipsis-symbolic")
            .tooltip_text("Download options")
            .build();
        more.add_css_class("flat");
        let popover = gtk::Popover::new();
        let details = gtk::Box::new(gtk::Orientation::Vertical, 10);
        details.set_margin_top(12);
        details.set_margin_bottom(12);
        details.set_margin_start(12);
        details.set_margin_end(12);
        details.append(&date);
        let speed_options = gtk::Box::new(gtk::Orientation::Vertical, 6);
        speed_options.append(&crate::download_ui::label(
            "Speed limit (KiB/s · 0 = unlimited)",
            "downloads-detail",
        ));
        speed_options.append(&limit);
        speed_options.set_visible(job.completed.is_none());
        // Tie the explanatory label to the same completion visibility as its control.
        let weak_options = speed_options.downgrade();
        limit.connect_visible_notify(move |limit| {
            if let Some(options) = weak_options.upgrade() {
                options.set_visible(limit.is_visible());
            }
        });
        details.append(&speed_options);
        details.append(&remove);
        popover.set_child(Some(&details));
        more.set_popover(Some(&popover));
        actions.append(&more);
        row.append(&actions);
        self.widget.append(&row);
        self.rows.borrow_mut().push(Row {
            job,
            container: row,
            detail,
            size,
            status,
            progress,
            speed: limit.clone(),
            date,
            pause: pause.clone(),
            resume: resume.clone(),
            reveal: reveal.clone(),
            worker: None,
            queued,
            removed: false,
            tick: Instant::now(),
            bytes: 0,
            speed_sample: None,
        });
        let weak = Rc::downgrade(self);
        pause.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let mut rows = this.rows.borrow_mut();
                let row = &mut rows[index];
                row.queued = false;
                if let Some(worker) = &row.worker {
                    worker.control.stop.store(true, Ordering::Relaxed);
                    row.status.set_text("Pausing…");
                    row.pause.set_sensitive(false);
                } else {
                    row.status.set_text("Paused");
                    row.detail.set_text("Ready to resume");
                    row.pause.set_visible(false);
                    row.resume.set_visible(true);
                    row.progress.set_visible(true);
                }
            }
        });
        let weak = Rc::downgrade(self);
        resume.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let mut rows = this.rows.borrow_mut();
                let row = &mut rows[index];
                if row.worker.is_none() {
                    row.queued = true;
                    row.status.set_text("Pending");
                    row.resume.set_visible(false);
                    row.pause.set_visible(true);
                    row.pause.set_sensitive(true);
                    row.detail.set_text("Waiting to download");
                }
            }
        });
        let weak = Rc::downgrade(self);
        limit.connect_value_changed(move |value| {
            if let Some(this) = weak.upgrade() {
                let mut rows = this.rows.borrow_mut();
                let row = &mut rows[index];
                let speed = value.value() as u64 * 1024;
                if speed != 0 && speed < 16384 {
                    return;
                }
                row.job.limit = speed;
                if let Some(worker) = &row.worker {
                    worker.control.limit.store(speed, Ordering::Relaxed);
                }
                drop(rows);
                this.persist();
            }
        });
        let weak = Rc::downgrade(self);
        remove.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let mut rows = this.rows.borrow_mut();
                let row = &mut rows[index];
                if row.worker.is_some() {
                    row.detail
                        .set_text("Pause and wait for the worker before removing this job.");
                    return;
                }
                row.queued = false;
                row.removed = true;
                row.container.set_visible(false);
                drop(rows);
                this.persist();
            }
        });
        let weak = Rc::downgrade(self);
        reveal.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let path = this.rows.borrow()[index].job.destination.clone();
                if path
                    .symlink_metadata()
                    .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
                {
                    gtk::FileLauncher::new(Some(&gtk::gio::File::for_path(path)))
                        .open_containing_folder(
                            Some(&this.parent),
                            None::<&gtk::gio::Cancellable>,
                            |_| {},
                        );
                } else {
                    this.warn("The completed file is missing or is no longer a regular file.");
                }
            }
        });
    }
    fn poll(&self) {
        let mut changed = false;
        let mut rows = self.rows.borrow_mut();
        for row in rows.iter_mut() {
            if let Some(worker) = &mut row.worker {
                let received = worker.control.received.load(Ordering::Relaxed);
                let total = worker.control.total.load(Ordering::Relaxed);
                let elapsed = row.tick.elapsed().as_secs_f64().max(0.001);
                // Chunks can arrive seconds apart when rate limited. Average over a
                // longer interval instead of flashing zero between every chunk.
                if received < row.bytes {
                    row.bytes = received;
                    row.tick = Instant::now();
                    row.speed_sample = None;
                } else if elapsed >= 3.0 {
                    row.speed_sample = Some(received.saturating_sub(row.bytes) as f64 / elapsed);
                    row.tick = Instant::now();
                    row.bytes = received;
                }
                let speed = row.speed_sample.unwrap_or(0.0);
                row.job.received = Some(received);
                if total > 0 {
                    row.job.total = Some(total);
                }
                row.size.set_text(&received_text(received, row.job.total));
                row.progress.set_fraction(fraction(received, row.job.total));
                let state = worker.control.status.lock().unwrap().clone();
                let eta = if total > received && speed > 1.0 {
                    format!(
                        " · {}s remaining",
                        ((total - received) as f64 / speed) as u64
                    )
                } else {
                    String::new()
                };
                let activity =
                    if state.contains("Verifying") || state.contains("Recovering completed") {
                        "Finishing up…".to_owned()
                    } else if state.contains("Checking") {
                        "Preparing download…".to_owned()
                    } else if state.contains("Retrying") {
                        "Reconnecting…".to_owned()
                    } else if received == 0 && speed < 1.0 {
                        "Connecting…".to_owned()
                    } else if row.speed_sample.is_none() {
                        "Downloading…".to_owned()
                    } else {
                        format!("{}/s{eta}", size(speed as u64))
                    };
                row.detail.set_text(&activity);
                row.detail.set_tooltip_text(Some(&state));
                if worker.finished() {
                    worker.join();
                    let received = worker.control.received.load(Ordering::Relaxed);
                    let total = worker.control.total.load(Ordering::Relaxed);
                    row.job.received = Some(received);
                    if total > 0 {
                        row.job.total = Some(total);
                    }
                    row.size.set_text(&received_text(received, row.job.total));
                    row.progress.set_fraction(fraction(received, row.job.total));
                    let result = worker
                        .control
                        .result
                        .lock()
                        .unwrap()
                        .take()
                        .unwrap_or_else(|| Err("Worker ended without a result".into()));
                    row.pause.set_visible(false);
                    row.pause.set_sensitive(true);
                    row.queued = false;
                    match result {
                        Ok(digest) => {
                            row.job.completed = Some(digest.clone());
                            row.job.finished = Some(now());
                            let finished_bytes = worker.control.received.load(Ordering::Relaxed);
                            row.job.received = Some(finished_bytes);
                            row.job.total = Some(finished_bytes);
                            row.size
                                .set_text(&received_text(finished_bytes, row.job.total));
                            row.date
                                .set_text(&crate::download_ui::date(row.job.finished.unwrap()));
                            row.speed.set_visible(false);
                            row.status.set_text(if row.job.sha256.is_some() {
                                "Verified"
                            } else {
                                "Completed"
                            });
                            row.detail.set_text(if row.job.sha256.is_some() {
                                "Checksum verified"
                            } else {
                                "Download complete"
                            });
                            row.detail
                                .set_tooltip_text(Some(&format!("SHA-256: {digest}")));
                            row.progress.set_visible(false);
                            row.reveal.set_visible(true);
                        }
                        Err(error) => {
                            row.status.set_text(if error == "Paused" {
                                "Paused"
                            } else if error.contains("Verification failed")
                                || error.contains("corrupted")
                            {
                                "Verify failed"
                            } else {
                                "Interrupted"
                            });
                            row.detail.set_text(if error == "Paused" {
                                "Ready to resume"
                            } else {
                                "Couldn’t finish. Resume to try again."
                            });
                            row.progress.set_visible(true);
                            row.detail.set_tooltip_text(Some(&error));
                            row.resume.set_visible(true);
                        }
                    }
                    row.worker = None;
                    changed = true;
                }
            }
        }
        let mut slots =
            2usize.saturating_sub(rows.iter().filter(|row| row.worker.is_some()).count());
        for row in rows.iter_mut() {
            if slots > 0 && row.queued && row.worker.is_none() && !row.removed {
                row.worker = Some(Worker::start(row.job.clone()));
                row.queued = false;
                row.pause.set_visible(true);
                row.pause.set_sensitive(true);
                row.progress.set_visible(true);
                row.resume.set_visible(false);
                row.detail.set_text("Connecting…");
                row.status.set_text("Downloading");
                row.tick = Instant::now();
                row.bytes = 0;
                row.speed_sample = None;
                slots -= 1;
            }
        }
        drop(rows);
        if changed {
            self.persist();
        }
        if let Some(callback) = self.changed.borrow().as_ref() {
            callback();
        }
    }
    pub fn clear_completed(&self) {
        for row in self.rows.borrow_mut().iter_mut() {
            if row.job.completed.is_some() && row.worker.is_none() {
                row.removed = true;
                row.container.set_visible(false);
            }
        }
        self.persist();
    }
    pub fn on_changed(&self, callback: impl Fn() + 'static) {
        *self.changed.borrow_mut() = Some(Box::new(callback));
    }
    pub fn counts(&self) -> (usize, usize) {
        let rows = self.rows.borrow();
        (
            rows.iter().filter(|r| !r.removed).count(),
            rows.iter().filter(|r| r.worker.is_some()).count(),
        )
    }
    pub fn shutdown(&self) {
        if let Some(poll) = self.poll.borrow_mut().take() {
            poll.remove();
        }
        let mut rows = self.rows.borrow_mut();
        for row in rows.iter_mut() {
            if let Some(worker) = &row.worker {
                worker.control.stop.store(true, Ordering::Relaxed);
            }
        }
        for row in rows.iter_mut() {
            if let Some(mut worker) = row.worker.take() {
                worker.join();
                row.job.received = Some(worker.control.received.load(Ordering::Relaxed));
                let total = worker.control.total.load(Ordering::Relaxed);
                if total > 0 {
                    row.job.total = Some(total);
                }
                if let Some(Ok(digest)) = worker.control.result.lock().unwrap().take() {
                    row.job.completed = Some(digest);
                    row.job.finished = Some(now());
                    row.job.received = Some(worker.control.received.load(Ordering::Relaxed));
                }
            }
            row.queued = false;
        }
        drop(rows);
        if self.enabled.get() {
            self.persist();
        }
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
fn fraction(received: u64, total: Option<u64>) -> f64 {
    total.filter(|total| *total > 0).map_or(0.0, |total| {
        (received as f64 / total as f64).clamp(0.0, 1.0)
    })
}
fn received_text(received: u64, total: Option<u64>) -> String {
    total.filter(|total| *total > 0).map_or_else(
        || format!("{} received", size(received)),
        |total| format!("{} / {}", size(received), size(total)),
    )
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}
