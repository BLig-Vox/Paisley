use browny::profile::{Profile, Settings};
use gtk::prelude::*;

fn contents() -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content
}

use std::{cell::RefCell, path::Path, rc::Rc};

pub(crate) struct SettingsEditor {
    window: gtk::glib::WeakRef<gtk::Window>,
    base: RefCell<Settings>,
    homepage: gtk::Entry,
    checks: Vec<gtk::CheckButton>,
    exceptions: gtk::Label,
    message: gtk::Label,
}

impl SettingsEditor {
    fn draft(&self) -> Settings {
        Settings {
            homepage: self.homepage.text().to_string(),
            restore_session: self.checks[0].is_active(),
            remember_history: self.checks[1].is_active(),
            show_memory: self.checks[2].is_active(),
            block_trackers: self.checks[3].is_active(),
            strict_privacy: self.checks[4].is_active(),
            ..self.base.borrow().clone()
        }
    }
    fn set_draft(&self, settings: Settings) {
        self.homepage.set_text(&settings.homepage);
        for (check, active) in self.checks.iter().zip([
            settings.restore_session,
            settings.remember_history,
            settings.show_memory,
            settings.block_trackers,
            settings.strict_privacy,
        ]) {
            check.set_active(active);
        }
        let mut text = settings
            .blocking_exceptions
            .iter()
            .take(128)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            text = "No site blocking exceptions".into();
        }
        if settings.blocking_exceptions.len() > 128 {
            text.push_str("\nAdditional existing exceptions unchanged, not shown.");
        }
        self.exceptions.set_text(&text);
        *self.base.borrow_mut() = settings;
    }
    pub(crate) fn import_file(&self, path: &Path) -> Result<(), String> {
        let settings = browny::settings_transfer::load_file(path, &self.draft())?;
        self.set_draft(settings);
        self.message.set_text("Imported into the draft only. Review homepage, switches and site exceptions, then Save to apply. Closing without Save discards changes.");
        Ok(())
    }
    pub(crate) fn export_file(&self, path: &Path) -> Result<(), String> {
        browny::settings_transfer::save_export(path, &self.draft())?;
        self.message.set_text("Exported supported switches only; homepage, site exceptions, paths, history and credentials were omitted. This is not a complete profile backup.");
        Ok(())
    }
}

pub(crate) fn show(
    parent: &gtk::ApplicationWindow,
    current: Settings,
    save: impl Fn(Settings) -> Result<(), String> + 'static,
) -> Rc<SettingsEditor> {
    let content = contents();
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search supported settings")
        .build();
    content.append(&search);
    let homepage_row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    homepage_row.append(&gtk::Label::new(Some(
        "Startup / search: homepage (HTTP or HTTPS)",
    )));
    let homepage = gtk::Entry::builder()
        .input_purpose(gtk::InputPurpose::Url)
        .hexpand(true)
        .build();
    homepage.set_widget_name("settings-homepage");
    homepage_row.append(&homepage);
    content.append(&homepage_row);
    let mut rows: Vec<(gtk::Widget, String)> =
        vec![(homepage_row.upcast(), "startup search homepage url".into())];
    let mut checks = Vec::new();
    for label in [
        "Startup: restore tabs after restarting this profile",
        "Privacy: remember visited pages (does not clear existing history)",
        "Appearance / performance: show partial process memory in title and status",
        "Blocking: bundled ad/tracker starter rules",
        "Privacy: Strict disables WebRTC and declines third-party cookies",
    ] {
        let check = gtk::CheckButton::with_label(label);
        content.append(&check);
        rows.push((check.clone().upcast(), label.to_lowercase()));
        checks.push(check);
    }
    search.connect_search_changed(move |search| {
        let query = search.text().to_lowercase();
        for (widget, keywords) in &rows {
            widget.set_visible(keywords.contains(query.as_str()));
        }
    });
    let exceptions = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .build();
    content.append(
        &gtk::Expander::builder()
            .label("Site blocking exceptions in this draft")
            .child(&exceptions)
            .build(),
    );
    content.append(&gtk::Label::builder().label("These supported settings are profile-local and apply on Save, without restart, to new requests and future startup. Existing requests are not undone. WebKit tracking prevention stays enabled. Strict can break calls/logins and is not anonymity. Sensitive permissions remain denied. Private settings stay in memory. Unsupported proxy, fingerprinting and permission-grant controls are not exposed.").wrap(true).xalign(0.0).build());
    content.append(&gtk::Label::builder().label("Import/reset edits this draft only. Export writes a user-selected file, including in private mode, but omits site URLs, paths and secrets by default. Exported files survive closing a private window. No import enables automation or grants permissions.").wrap(true).xalign(0.0).build());
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let import = gtk::Button::with_label("Import preferences…");
    let export = gtk::Button::with_label("Export preferences…");
    let reset = gtk::Button::with_label("Reset draft…");
    for button in [&import, &export, &reset] {
        actions.append(button);
    }
    content.append(&actions);
    let message = gtk::Label::builder().wrap(true).xalign(0.0).build();
    let save_button = gtk::Button::with_label("Save settings");
    save_button.add_css_class("suggested-action");
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&content)
        .build();
    let footer = contents();
    footer.set_margin_top(8);
    footer.set_margin_bottom(12);
    footer.append(&message);
    footer.append(&save_button);
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    layout.append(&scroll);
    layout.append(&footer);
    let window = gtk::Window::builder()
        .title("Browny settings")
        .transient_for(parent)
        .destroy_with_parent(true)
        .modal(true)
        .default_width(620)
        .default_height(680)
        .child(&layout)
        .build();
    let editor = Rc::new(SettingsEditor {
        window: window.downgrade(),
        base: RefCell::new(current.clone()),
        homepage,
        checks,
        exceptions,
        message,
    });
    editor.set_draft(current);
    let retained = Rc::clone(&editor);
    // Window owns the editor; the reverse window reference is weak.
    window.connect_close_request(move |_| {
        let _keep = &retained;
        gtk::glib::Propagation::Proceed
    });
    let weak = Rc::downgrade(&editor);
    save_button.connect_clicked(move |_| {
        let Some(editor) = weak.upgrade() else {
            return;
        };
        match save(editor.draft()) {
            Ok(()) => {
                if let Some(window) = editor.window.upgrade() {
                    window.close();
                }
            }
            Err(message) => editor.message.set_text(&message),
        }
    });
    let weak = Rc::downgrade(&editor);
    reset.connect_clicked(move |_| {
        let Some(editor) = weak.upgrade() else { return; };
        let Some(window) = editor.window.upgrade() else { return; };
        let dialog = gtk::AlertDialog::builder().message("Reset settings draft to defaults?").detail("On Save, defaults enable history recording, session restoration, memory display and starter blocking; Strict is off, homepage becomes DuckDuckGo and site blocking exceptions are cleared. Existing history, bookmarks, cookies and files are not deleted. Cancel keeps your draft unchanged.").buttons(["Cancel", "Reset draft"]).cancel_button(0).default_button(0).modal(true).build();
        let weak = Rc::downgrade(&editor);
        dialog.choose(Some(&window), None::<&gtk::gio::Cancellable>, move |response| {
            if response == Ok(1) && let Some(editor) = weak.upgrade() {
                editor.set_draft(Settings::default());
                editor.message.set_text("Defaults loaded into draft, including history recording ON. Save to apply, or close without saving to discard.");
            }
        });
    });
    for (button, importing) in [(import, true), (export, false)] {
        let weak = Rc::downgrade(&editor);
        button.connect_clicked(move |_| {
            let Some(editor) = weak.upgrade() else {
                return;
            };
            let Some(window) = editor.window.upgrade() else {
                return;
            };
            let dialog = gtk::FileDialog::builder()
                .title(if importing {
                    "Import preferences into draft"
                } else {
                    "Export supported preferences (no site URLs)"
                })
                .modal(true)
                .build();
            if !importing {
                dialog.set_initial_name(Some("browny-preferences.json"));
            }
            let weak = Rc::downgrade(&editor);
            let finish = move |result: Result<gtk::gio::File, gtk::glib::Error>| {
                let Some(editor) = weak.upgrade() else {
                    return;
                };
                if !editor
                    .window
                    .upgrade()
                    .is_some_and(|window| window.is_visible())
                {
                    return;
                }
                match result {
                    Ok(file) => {
                        let result = file
                            .path()
                            .ok_or_else(|| "Select a local file".to_string())
                            .and_then(|path| {
                                if importing {
                                    editor.import_file(&path)
                                } else {
                                    editor.export_file(&path)
                                }
                            });
                        if let Err(message) = result {
                            editor.message.set_text(&message);
                        }
                    }
                    Err(error)
                        if error.matches(gtk::DialogError::Dismissed)
                            || error.matches(gtk::DialogError::Cancelled)
                            || error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
                    Err(error) => editor
                        .message
                        .set_text(&format!("Native file chooser failed: {error}")),
                }
            };
            if importing {
                dialog.open(Some(&window), None::<&gtk::gio::Cancellable>, finish);
            } else {
                dialog.save(Some(&window), None::<&gtk::gio::Cancellable>, finish);
            }
        });
    }
    window.present();
    editor
}

pub fn profiles(parent: &gtk::ApplicationWindow, current: &str, is_private: bool) {
    let catalog = if is_private {
        Ok(browny::profile_catalog::ProfileCatalog::at(
            Path::new(""),
            true,
        ))
    } else {
        browny::profile_catalog::ProfileCatalog::system(false)
    };
    profiles_with_catalog(parent, current, is_private, catalog);
}

pub(crate) fn profiles_with_catalog(
    parent: &gtk::ApplicationWindow,
    current: &str,
    is_private: bool,
    catalog: Result<browny::profile_catalog::ProfileCatalog, String>,
) -> gtk::Window {
    let content = contents();
    let label = gtk::Label::builder().label(format!("Current profile: {current}\nOpening another profile activates its own isolated browser window. An unused ID creates a new profile.")).wrap(true).xalign(0.0).build();
    content.append(&label);
    let entry = gtk::Entry::builder()
        .placeholder_text("Profile ID: work, personal, testing…")
        .build();
    content.append(&entry);
    let open = gtk::Button::with_label("Open / switch profile");
    content.append(&open);
    let private = gtk::Button::with_label("Open a private window");
    content.append(&private);
    let error = gtk::Label::builder().wrap(true).xalign(0.0).build();
    content.append(&error);
    let window = gtk::Window::builder()
        .title("Browny profiles")
        .transient_for(parent)
        .destroy_with_parent(true)
        .modal(true)
        .default_width(480)
        .child(&content)
        .build();
    if is_private {
        content.append(&gtk::Label::new(Some(
            "Profile management is unavailable in private windows.",
        )));
    } else {
        content.append(&gtk::Label::builder().label("Rename changes only the display name, not the stable ID or storage paths. Close a profile before renaming/deleting it. Deletion is permanent and requires confirmation.").wrap(true).xalign(0.0).build());
        match catalog {
            Ok(catalog) => {
                let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
                let scroll = gtk::ScrolledWindow::builder()
                    .min_content_height(120)
                    .max_content_height(300)
                    .propagate_natural_height(true)
                    .child(&list)
                    .build();
                content.append(&scroll);
                render_profiles(&list, &window, &error, &catalog);
            }
            Err(message) => error.set_text(&message),
        }
    }
    let weak = window.downgrade();
    let error_open = error.clone();
    open.connect_clicked(move |_| {
        let id = entry.text();
        let result = Profile::validate_id(&id).and_then(|()| spawn_profile(&id, false));
        match result {
            Ok(()) => {
                if let Some(window) = weak.upgrade() {
                    window.close();
                }
            }
            Err(message) => error_open.set_text(&message),
        }
    });
    let weak = window.downgrade();
    private.connect_clicked(move |_| match spawn_profile("private", true) {
        Ok(()) => {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        }
        Err(message) => error.set_text(&message),
    });
    window.present();
    window
}

fn render_profiles(
    list: &gtk::Box,
    window: &gtk::Window,
    error: &gtk::Label,
    catalog: &browny::profile_catalog::ProfileCatalog,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let entries = match catalog.list() {
        Ok(entries) => entries,
        Err(message) => {
            error.set_text(&message);
            return;
        }
    };
    for profile in entries {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
        row.append(
            &gtk::Label::builder()
                .label(format!("Profile ID: {}", profile.id))
                .xalign(0.0)
                .selectable(true)
                .build(),
        );
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let name = gtk::Entry::builder()
            .text(&profile.name)
            .hexpand(true)
            .build();
        controls.append(&name);
        let rename = gtk::Button::with_label("Rename");
        controls.append(&rename);
        let delete = gtk::Button::with_label("Delete…");
        controls.append(&delete);
        row.append(&controls);
        list.append(&row);
        let open = gtk::Button::with_label("Open");
        controls.append(&open);
        let weak_window = window.downgrade();
        let open_error = error.clone();
        let open_id = profile.id.clone();
        open.connect_clicked(move |_| match spawn_profile(&open_id, false) {
            Ok(()) => {
                if let Some(window) = weak_window.upgrade() {
                    window.close();
                }
            }
            Err(message) => open_error.set_text(&message),
        });
        let (weak_list, weak_window, error, catalog, id) = (
            list.downgrade(),
            window.downgrade(),
            error.clone(),
            catalog.clone(),
            profile.id.clone(),
        );
        let rename_list = weak_list.clone();
        let rename_window = weak_window.clone();
        let rename_error = error.clone();
        let rename_catalog = catalog.clone();
        let rename_id = id.clone();
        rename.connect_clicked(move |_| {
            let (Some(list), Some(window)) = (rename_list.upgrade(), rename_window.upgrade())
            else {
                return;
            };
            match rename_catalog.rename(&rename_id, &name.text()) {
                Ok(()) => {
                    rename_error.set_text("");
                    render_profiles(&list, &window, &rename_error, &rename_catalog);
                }
                Err(message) => rename_error.set_text(&message),
            }
        });
        delete.connect_clicked(move |_| {
            let Some(window) = weak_window.upgrade() else { return; };
            let dialog = gtk::AlertDialog::builder().message(format!("Delete profile {id}?")).detail("Permanently deletes this profile's settings, bookmarks, history, session, cookies, website data, download records and cache. Downloaded files outside the profile are not deleted. Deletion across folders may only partly succeed. Close the profile before deleting it.").buttons(["Cancel", "Delete profile"]).cancel_button(0).default_button(0).modal(true).build();
            let (weak_list, weak_window, error, catalog, id) = (weak_list.clone(), weak_window.clone(), error.clone(), catalog.clone(), id.clone());
            dialog.choose(Some(&window), None::<&gtk::gio::Cancellable>, move |choice| {
                if choice != Ok(1) { return; }
                let (Some(list), Some(window)) = (weak_list.upgrade(), weak_window.upgrade()) else { return; };
                if !window.is_visible() { return; }
                match catalog.delete(&id, true) {
                    Ok(()) => { error.set_text(""); render_profiles(&list, &window, &error, &catalog); }
                    Err(message) => error.set_text(&message),
                }
            });
        });
    }
}

fn spawn_profile(id: &str, private: bool) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new(executable);
    command.args(["--profile", id]);
    if private {
        command.arg("--private");
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    // Reap the launcher process when activating an existing profile, too.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
