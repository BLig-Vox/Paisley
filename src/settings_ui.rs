use gtk::prelude::*;
use paisley::profile::{Profile, Settings};

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
        let settings = paisley::settings_transfer::load_file(path, &self.draft())?;
        self.set_draft(settings);
        self.message.set_text("Imported into the draft only. Review homepage, switches and site exceptions, then Save to apply. Closing without Save discards changes.");
        Ok(())
    }
    pub(crate) fn export_file(&self, path: &Path) -> Result<(), String> {
        paisley::settings_transfer::save_export(path, &self.draft())?;
        self.message.set_text("Exported supported switches only; homepage, site exceptions, paths, history and credentials were omitted. This is not a complete profile backup.");
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn show(
    parent: &gtk::ApplicationWindow,
    current: Settings,
    save: impl Fn(Settings) -> Result<(), String> + 'static,
) -> Rc<SettingsEditor> {
    build(parent, current, save, None)
}

pub(crate) fn show_in_sheet(
    parent: &gtk::ApplicationWindow,
    current: Settings,
    save: impl Fn(Settings) -> Result<(), String> + 'static,
    menu: &Rc<crate::menu_sheet::MenuSheet>,
) -> Rc<SettingsEditor> {
    build(parent, current, save, Some(menu))
}

fn build(
    parent: &gtk::ApplicationWindow,
    current: Settings,
    save: impl Fn(Settings) -> Result<(), String> + 'static,
    host: Option<&Rc<crate::menu_sheet::MenuSheet>>,
) -> Rc<SettingsEditor> {
    let content = contents();
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search supported settings")
        .build();
    content.append(&search);
    let homepage_row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    homepage_row.append(&crate::download_ui::label("Homepage", "downloads-filename"));
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
        "Restore tabs on startup",
        "Remember browsing history",
        "Show memory usage",
        "Block ads and trackers",
        "Strict privacy",
    ] {
        let check = gtk::CheckButton::with_label(label);
        if host.is_some() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.add_css_class("setting-row");
            let text = crate::download_ui::label(label, "setting-label");
            let toggle = gtk::Switch::builder()
                .valign(gtk::Align::Center)
                .tooltip_text(label)
                .build();
            text.set_mnemonic_widget(Some(&toggle));
            check
                .bind_property("active", &toggle, "active")
                .bidirectional()
                .sync_create()
                .build();
            row.append(&text);
            row.append(&toggle);
            content.append(&row);
            rows.push((row.upcast(), label.to_lowercase()));
        } else {
            content.append(&check);
            rows.push((check.clone().upcast(), label.to_lowercase()));
        }
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
            .label("Site exceptions")
            .child(&exceptions)
            .build(),
    );
    let notes = gtk::Box::new(gtk::Orientation::Vertical, 8);
    #[cfg(not(feature="cef"))]
    notes.append(&gtk::Label::builder().label("Changes apply to this profile when saved. Existing history is kept when recording is disabled. Strict privacy disables WebRTC and third-party cookies and can affect calls or sign-ins. Private settings remain in memory.").wrap(true).xalign(0.0).build());
    #[cfg(feature="cef")]
    notes.append(&gtk::Label::builder().label("Changes apply to this profile when saved. Strict privacy blocks third-party cookies and removes WebRTC APIs from newly opened tabs. Reopen existing tabs after changing strict privacy. Private settings remain in memory.").wrap(true).xalign(0.0).build());
    notes.append(&gtk::Label::builder().label("Import and reset change the draft only. Export saves supported preferences without site URLs or secrets. Exported files remain after a private window closes.").wrap(true).xalign(0.0).build());
    content.append(
        &gtk::Expander::builder()
            .label("Advanced")
            .child(&notes)
            .build(),
    );
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let import = gtk::Button::with_label("Import preferences…");
    let export = gtk::Button::with_label("Export preferences…");
    let reset = gtk::Button::with_label("Reset draft…");
    for button in [&import, &export, &reset] {
        actions.append(button);
    }
    notes.append(&actions);
    let message = gtk::Label::builder().wrap(true).xalign(0.0).build();
    crate::download_ui::hide_when_empty(&message);
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
    let window: gtk::Window = if host.is_some() {
        parent.clone().upcast()
    } else {
        gtk::Window::builder()
            .title("Paisley settings")
            .transient_for(parent)
            .destroy_with_parent(true)
            .modal(true)
            .default_width(620)
            .default_height(680)
            .child(&layout)
            .build()
    };
    if host.is_some() {
        for body in [&content, &footer] {
            body.set_margin_start(0);
            body.set_margin_end(0);
            body.set_margin_top(0);
            body.set_margin_bottom(0);
        }
    }
    let finish = finish_callback(&window, host);
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
    if host.is_none() {
        window.connect_close_request(move |_| {
            let _keep = &retained;
            gtk::glib::Propagation::Proceed
        });
    }
    let weak = Rc::downgrade(&editor);
    save_button.connect_clicked(move |_| {
        let Some(editor) = weak.upgrade() else {
            return;
        };
        match save(editor.draft()) {
            Ok(()) => {
                finish();
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
                dialog.set_initial_name(Some("paisley-preferences.json"));
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
    if let Some(host) = host {
        host.show_page("Settings", &layout, editor.clone());
    } else {
        window.present();
    }
    editor
}

fn finish_callback(
    window: &gtk::Window,
    host: Option<&Rc<crate::menu_sheet::MenuSheet>>,
) -> Rc<dyn Fn()> {
    if let Some(host) = host {
        let weak = Rc::downgrade(host);
        Rc::new(move || {
            if let Some(host) = weak.upgrade() {
                host.back();
            }
        })
    } else {
        let weak = window.downgrade();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
        })
    }
}

#[cfg(test)]
pub fn profiles(parent: &gtk::ApplicationWindow, current: &str, is_private: bool) {
    let catalog = if is_private {
        Ok(paisley::profile_catalog::ProfileCatalog::at(
            Path::new(""),
            true,
        ))
    } else {
        paisley::profile_catalog::ProfileCatalog::system(false)
    };
    profiles_with_catalog(parent, current, is_private, catalog);
}

pub fn profiles_in_sheet(
    parent: &gtk::ApplicationWindow,
    current: &str,
    is_private: bool,
    menu: &Rc<crate::menu_sheet::MenuSheet>,
) {
    let catalog = if is_private {
        Ok(paisley::profile_catalog::ProfileCatalog::at(
            Path::new(""),
            true,
        ))
    } else {
        paisley::profile_catalog::ProfileCatalog::system(false)
    };
    build_profiles(parent, current, is_private, catalog, Some(menu));
}

#[cfg(test)]
pub(crate) fn profiles_with_catalog(
    parent: &gtk::ApplicationWindow,
    current: &str,
    is_private: bool,
    catalog: Result<paisley::profile_catalog::ProfileCatalog, String>,
) -> gtk::Window {
    build_profiles(parent, current, is_private, catalog, None)
}

fn build_profiles(
    parent: &gtk::ApplicationWindow,
    current: &str,
    is_private: bool,
    catalog: Result<paisley::profile_catalog::ProfileCatalog, String>,
    host: Option<&Rc<crate::menu_sheet::MenuSheet>>,
) -> gtk::Window {
    let content = contents();
    let intro = gtk::Label::builder()
        .label("Separate spaces for work, personal browsing, and everything else.")
        .wrap(true)
        .xalign(0.0)
        .build();
    intro.add_css_class("downloads-detail");
    content.append(&intro);
    let window: gtk::Window = if host.is_some() {
        parent.clone().upcast()
    } else {
        gtk::Window::builder()
            .title("Paisley profiles")
            .transient_for(parent)
            .destroy_with_parent(true)
            .modal(true)
            .default_width(480)
            .child(&content)
            .build()
    };
    let finish = finish_callback(&window, host);
    if host.is_some() {
        content.set_margin_start(0);
        content.set_margin_end(0);
        content.set_margin_top(0);
        content.set_margin_bottom(0);
    }
    let error = gtk::Label::builder().wrap(true).xalign(0.0).build();
    error.add_css_class("error");
    crate::download_ui::hide_when_empty(&error);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let create = gtk::ToggleButton::with_label("Create profile");
    create.add_css_class("suggested-action");
    create.set_hexpand(true);
    let private = gtk::Button::with_label("Private window");
    private.set_hexpand(true);
    actions.append(&create);
    actions.append(&private);
    content.append(&actions);
    let form = gtk::Box::new(gtk::Orientation::Vertical, 10);
    form.add_css_class("profile-create");
    form.append(&crate::download_ui::label(
        "New profile",
        "downloads-filename",
    ));
    let entry = gtk::Entry::builder()
        .placeholder_text("Profile ID, e.g. work")
        .max_length(64)
        .build();
    entry.set_tooltip_text(Some(
        "Use letters, digits, hyphens or underscores. You can change its display name later.",
    ));
    form.append(&entry);
    let helper = gtk::Label::builder()
        .label("Opens in a separate window with its own history, bookmarks, and sign-ins.")
        .wrap(true)
        .xalign(0.0)
        .build();
    helper.add_css_class("downloads-detail");
    form.append(&helper);
    let create_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let cancel = gtk::Button::with_label("Cancel");
    let open = gtk::Button::with_label("Create and open");
    open.add_css_class("suggested-action");
    open.set_hexpand(true);
    create_actions.append(&cancel);
    create_actions.append(&open);
    form.append(&create_actions);
    let reveal = gtk::Revealer::builder()
        .child(&form)
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .transition_duration(120)
        .build();
    create
        .bind_property("active", &reveal, "reveal-child")
        .sync_create()
        .build();
    let weak_entry = entry.downgrade();
    create.connect_toggled(move |button| {
        if button.is_active()
            && let Some(entry) = weak_entry.upgrade()
        {
            entry.grab_focus();
        }
    });
    let weak_create = create.downgrade();
    let cancel_entry = entry.downgrade();
    let cancel_error = error.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(create) = weak_create.upgrade() {
            create.set_active(false);
        }
        if let Some(entry) = cancel_entry.upgrade() {
            entry.set_text("");
        }
        if let Some(error) = cancel_error.upgrade() {
            error.set_text("");
        }
    });
    let weak_error = error.downgrade();
    entry.connect_changed(move |_| {
        if let Some(error) = weak_error.upgrade() {
            error.set_text("");
        }
    });
    let weak_open = open.downgrade();
    entry.connect_activate(move |_| {
        if let Some(open) = weak_open.upgrade() {
            open.emit_clicked();
        }
    });
    content.append(&reveal);
    content.append(&error);
    let query = gtk::SearchEntry::builder()
        .placeholder_text("Find a profile")
        .build();
    content.append(&query);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.set_vexpand(true);
    content.append(&list);
    let catalog = match catalog {
        Ok(catalog) if !is_private => Some(catalog),
        Ok(_) => None,
        Err(message) => {
            error.set_text(&message);
            None
        }
    };
    create.set_sensitive(catalog.is_some());
    query.set_sensitive(catalog.is_some());
    if let Some(catalog) = catalog.clone() {
        render_profiles(&list, &window, &error, &catalog, &finish, current, "");
        let (weak_list, weak_window, weak_error, finish, current) = (
            list.downgrade(),
            window.downgrade(),
            error.downgrade(),
            finish.clone(),
            current.to_owned(),
        );
        query.connect_search_changed(move |query| {
            if let (Some(list), Some(window), Some(error)) = (
                weak_list.upgrade(),
                weak_window.upgrade(),
                weak_error.upgrade(),
            ) {
                render_profiles(
                    &list,
                    &window,
                    &error,
                    &catalog,
                    &finish,
                    &current,
                    &query.text(),
                );
            }
        });
    } else if is_private {
        let notice=gtk::Label::builder().label("Private browsing keeps this session temporary. Saved profiles are managed from a regular window.").wrap(true).xalign(0.0).build();
        notice.add_css_class("downloads-detail");
        list.append(&notice);
    }
    let (done, error_open) = (finish.clone(), error.clone());
    open.connect_clicked(move |_| {
        let id = entry.text().trim().to_owned();
        let result = Profile::validate_id(&id)
            .map_err(|_| {
                "Use 1–64 letters, digits, hyphens, or underscores for the profile ID.".to_owned()
            })
            .and_then(|()| {
                let catalog = catalog
                    .as_ref()
                    .ok_or("Profile management is unavailable")?;
                if catalog.list()?.iter().any(|profile| profile.id == id) {
                    return Err("That profile already exists. Open it from the list below.".into());
                }
                spawn_profile(&id, false)
            });
        match result {
            Ok(()) => done(),
            Err(message) => error_open.set_text(&message),
        }
    });
    let done = finish.clone();
    private.connect_clicked(move |_| match spawn_profile("private", true) {
        Ok(()) => done(),
        Err(message) => error.set_text(&message),
    });
    if let Some(host) = host {
        let scroll = gtk::ScrolledWindow::builder()
            .child(&content)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        host.show_page("Profiles", &scroll, ());
    } else {
        window.present();
    }
    window
}

fn render_profiles(
    list: &gtk::Box,
    window: &gtk::Window,
    error: &gtk::Label,
    catalog: &paisley::profile_catalog::ProfileCatalog,
    finish: &Rc<dyn Fn()>,
    current: &str,
    query: &str,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let mut entries = match catalog.list() {
        Ok(entries) => entries,
        Err(message) => {
            error.set_text(&message);
            return;
        }
    };
    entries.sort_by(|a, b| {
        (a.id != current, a.name.to_lowercase()).cmp(&(b.id != current, b.name.to_lowercase()))
    });
    let query = query.to_lowercase();
    entries.retain(|profile| {
        profile.name.to_lowercase().contains(&query) || profile.id.to_lowercase().contains(&query)
    });
    if entries.is_empty() {
        let empty = gtk::Label::builder()
            .label(if query.is_empty() {
                "No saved profiles yet"
            } else {
                "No matching profiles"
            })
            .margin_top(24)
            .margin_bottom(24)
            .build();
        empty.add_css_class("dim-label");
        list.append(&empty);
    }
    for profile in entries {
        let active = profile.id == current;
        let row = gtk::Box::new(gtk::Orientation::Vertical, 10);
        row.add_css_class("profile-row");
        let identity = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let initial = profile
            .name
            .chars()
            .next()
            .unwrap_or('P')
            .to_uppercase()
            .collect::<String>();
        let avatar = gtk::Label::builder()
            .label(initial)
            .width_request(38)
            .height_request(38)
            .valign(gtk::Align::Center)
            .build();
        avatar.add_css_class("profile-avatar");
        identity.append(&avatar);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
        text.set_hexpand(true);
        text.append(&crate::download_ui::label(
            &profile.name,
            "downloads-filename",
        ));
        let detail = crate::download_ui::label(&profile.id, "downloads-detail");
        detail.set_tooltip_text(Some("Stable profile ID"));
        text.append(&detail);
        identity.append(&text);
        if active {
            let badge = gtk::Label::new(Some("Current"));
            badge.add_css_class("profile-current");
            identity.append(&badge);
        } else {
            let open = gtk::Button::with_label("Open");
            open.set_valign(gtk::Align::Center);
            let (done, error, id) = (finish.clone(), error.clone(), profile.id.clone());
            open.connect_clicked(move |_| match spawn_profile(&id, false) {
                Ok(()) => done(),
                Err(message) => error.set_text(&message),
            });
            identity.append(&open);
        }
        let more = gtk::ToggleButton::new();
        more.set_child(Some(&gtk::Image::from_icon_name(
            "lucide-ellipsis-symbolic",
        )));
        more.add_css_class("flat");
        more.set_valign(gtk::Align::Center);
        more.set_tooltip_text(Some("Profile options"));
        identity.append(&more);
        row.append(&identity);
        let options = gtk::Box::new(gtk::Orientation::Vertical, 10);
        options.add_css_class("profile-options");
        let name = gtk::Entry::builder()
            .text(&profile.name)
            .max_length(128)
            .build();
        name.set_sensitive(!active);
        name.set_tooltip_text(Some("Display name"));
        options.append(&crate::download_ui::label(
            "Display name",
            "downloads-detail",
        ));
        options.append(&name);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let rename = gtk::Button::with_label("Save name");
        rename.set_sensitive(!active);
        let delete = gtk::Button::with_label("Delete profile…");
        delete.add_css_class("destructive-action");
        delete.set_sensitive(!active);
        buttons.append(&rename);
        buttons.append(&delete);
        options.append(&buttons);
        let note=gtk::Label::builder().label(if active {"Close this profile before renaming or deleting it."}else{"Renaming keeps the profile ID and browsing data. Close the profile before editing it."}).wrap(true).xalign(0.0).build();
        note.add_css_class("downloads-detail");
        options.append(&note);
        let reveal = gtk::Revealer::builder()
            .child(&options)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(120)
            .build();
        more.bind_property("active", &reveal, "reveal-child")
            .sync_create()
            .build();
        row.append(&reveal);
        list.append(&row);
        let (weak_list, weak_window, error, catalog, id) = (
            list.downgrade(),
            window.downgrade(),
            error.clone(),
            catalog.clone(),
            profile.id.clone(),
        );
        let (
            rename_list,
            rename_window,
            rename_error,
            rename_catalog,
            rename_id,
            rename_finish,
            rename_current,
            rename_query,
        ) = (
            weak_list.clone(),
            weak_window.clone(),
            error.clone(),
            catalog.clone(),
            id.clone(),
            finish.clone(),
            current.to_owned(),
            query.clone(),
        );
        rename.connect_clicked(move |_| {
            let (Some(list), Some(window)) = (rename_list.upgrade(), rename_window.upgrade())
            else {
                return;
            };
            if !list.is_mapped() {
                return;
            }
            match rename_catalog.rename(&rename_id, name.text().trim()) {
                Ok(()) => {
                    rename_error.set_text("");
                    render_profiles(
                        &list,
                        &window,
                        &rename_error,
                        &rename_catalog,
                        &rename_finish,
                        &rename_current,
                        &rename_query,
                    );
                }
                Err(message) => rename_error.set_text(&message),
            }
        });
        let (finish, current, query) = (finish.clone(), current.to_owned(), query.clone());
        delete.connect_clicked(move |_|{
            let Some(parent)=weak_window.upgrade()else{return;};
            let dialog=gtk::AlertDialog::builder().message(format!("Delete profile {id}?"))
                .detail("Permanently deletes this profile’s settings, history, bookmarks, cookies, sessions, download records, and cache. Downloaded files outside the profile are kept. Close the profile first. Deletion across folders may only partly succeed.")
                .buttons(["Cancel","Delete profile"]).cancel_button(0).default_button(0).modal(true).build();
            let (list,window,error,catalog,id,finish,current,query)=(weak_list.clone(),weak_window.clone(),error.clone(),catalog.clone(),id.clone(),finish.clone(),current.clone(),query.clone());
            dialog.choose(Some(&parent),None::<&gtk::gio::Cancellable>,move |choice|{
                if choice!=Ok(1){return;}
                let (Some(list),Some(window))=(list.upgrade(),window.upgrade())else{return;};
                if !list.is_mapped()||!window.is_visible(){return;}
                match catalog.delete(&id,true){
                    Ok(())=>{error.set_text("");render_profiles(&list,&window,&error,&catalog,&finish,&current,&query);},
                    Err(message)=>error.set_text(&message),
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
