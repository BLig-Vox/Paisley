//! Docked browsing controls. Callbacks retain only weak browser references.
use gtk::prelude::*;
use std::rc::Rc;

type Operation = Rc<dyn Fn() -> Result<(), String>>;
pub type SaveBookmark = Rc<dyn Fn(&str) -> Result<(), String>>;

fn panel() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, 16)
}
fn detail(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .build();
    label.add_css_class("downloads-detail");
    label
}
fn site(uri: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("browsing-site");
    let icon = gtk::Image::from_icon_name("lucide-globe-symbolic");
    icon.set_pixel_size(22);
    row.append(&icon);
    let domain = url::Url::parse(uri)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned));
    let label = gtk::Label::builder()
        .label(domain.as_deref().unwrap_or("This page"))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .hexpand(true)
        .build();
    label.add_css_class("downloads-filename");
    row.append(&label);
    row.set_tooltip_text(Some(uri));
    row
}

pub fn protection(
    menu: &crate::menu_sheet::MenuSheet,
    uri: &str,
    enabled: bool,
    global: bool,
    available: bool,
    set_enabled: Rc<dyn Fn(bool) -> Result<(), String>>,
) {
    let root = panel();
    root.append(&site(uri));
    let supported = url::Url::parse(uri)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("setting-row");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 5);
    text.set_hexpand(true);
    let heading = gtk::Label::builder()
        .label("Block ads & trackers")
        .xalign(0.0)
        .build();
    heading.add_css_class("downloads-filename");
    text.append(&heading);
    let state = detail(if !supported {
        "Available on websites"
    } else if !global {
        "Turn on blocking in Settings first"
    } else if !available {
        "Protection is not ready"
    } else if enabled {
        "On for this site"
    } else {
        "Off for this site"
    });
    text.append(&state);
    row.append(&text);
    let toggle = gtk::Switch::new();
    toggle.set_active(enabled && supported);
    toggle.set_sensitive(supported && global && available);
    toggle.set_valign(gtk::Align::Center);
    toggle.set_tooltip_text(Some("Block ads and trackers on this site"));
    row.append(&toggle);
    root.append(&row);
    let feedback = detail("");
    crate::download_ui::hide_when_empty(&feedback);
    root.append(&feedback);
    root.append(&detail(
        "Uses Paisley’s starter blocking rules. Some ads and trackers may still appear.",
    ));
    let settings = gtk::Button::with_label("Protection settings");
    settings.add_css_class("flat");
    settings.set_halign(gtk::Align::Start);
    settings.set_action_name(Some("win.settings"));
    root.append(&settings);
    let status = state.downgrade();
    let feedback = feedback.downgrade();
    let resetting = std::cell::Cell::new(false);
    toggle.connect_state_set(move |switch, enabled| {
        if resetting.get() {
            return gtk::glib::Propagation::Stop;
        }
        match set_enabled(enabled) {
            Ok(()) => {
                switch.set_state(enabled);
                if let Some(label) = status.upgrade() {
                    label.set_text(if enabled {
                        "On for this site"
                    } else {
                        "Off for this site"
                    });
                }
                if let Some(label) = feedback.upgrade() {
                    label.remove_css_class("error");
                    label.set_text("Saved. Reload the page to apply to existing content.");
                }
            }
            Err(error) => {
                resetting.set(true);
                switch.set_active(switch.state());
                resetting.set(false);
                if let Some(label) = feedback.upgrade() {
                    label.add_css_class("error");
                    label.set_text(&error);
                }
            }
        }
        gtk::glib::Propagation::Stop
    });
    menu.show_page("Site protection", &root, ());
}

pub fn bookmark_form(
    uri: &str,
    title: &str,
    saved: bool,
    private: bool,
    save: SaveBookmark,
    remove: Operation,
) -> gtk::Box {
    let root = panel();
    let heading = gtk::Label::builder().label("Title").xalign(0.0).build();
    heading.add_css_class("downloads-filename");
    root.append(&heading);
    let entry = gtk::Entry::builder()
        .text(title)
        .max_length(256)
        .placeholder_text("Bookmark title")
        .hexpand(true)
        .build();
    root.append(&entry);
    let address = detail(uri);
    address.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    address.set_selectable(true);
    root.append(&address);
    let message = detail(if private {
        "Private bookmark · kept until this window closes"
    } else if saved {
        "Saved in this profile"
    } else {
        "Save this page in your profile"
    });
    crate::download_ui::hide_when_empty(&message);
    root.append(&message);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let save_button = gtk::Button::with_label(if saved {
        "Save changes"
    } else {
        "Save bookmark"
    });
    save_button.add_css_class("suggested-action");
    save_button.set_hexpand(true);
    let remove_button = gtk::Button::with_label("Remove");
    remove_button.add_css_class("flat");
    remove_button.set_visible(saved);
    actions.append(&save_button);
    actions.append(&remove_button);
    root.append(&actions);
    if paisley::library::Library::default()
        .bookmark(uri, title, 0)
        .is_err()
    {
        save_button.set_sensitive(false);
        message.set_text("Open a website to save a bookmark.");
    }
    let field = entry.downgrade();
    let feedback = message.downgrade();
    let remove_widget = remove_button.downgrade();
    save_button.connect_clicked(move |button| {
        let Some(entry) = field.upgrade() else {
            return;
        };
        let result = save(entry.text().as_str());
        if let Some(label) = feedback.upgrade() {
            match result {
                Ok(()) => {
                    label.remove_css_class("error");
                    label.set_text(if private {
                        "Saved for this private window"
                    } else {
                        "Bookmark saved"
                    });
                    button.set_label("Save changes");
                    if let Some(remove) = remove_widget.upgrade() {
                        remove.set_visible(true);
                    }
                }
                Err(error) => {
                    label.add_css_class("error");
                    label.set_text(&error);
                }
            }
        }
    });
    let submit = save_button.downgrade();
    entry.connect_activate(move |_| {
        if let Some(button) = submit.upgrade()
            && button.is_sensitive()
        {
            button.emit_clicked();
        }
    });
    let feedback = message.downgrade();
    let submit = save_button.downgrade();
    remove_button.connect_clicked(move |button| {
        let result = remove();
        if let Some(label) = feedback.upgrade() {
            match result {
                Ok(()) => {
                    label.remove_css_class("error");
                    label.set_text("Bookmark removed");
                    button.set_visible(false);
                    if let Some(save) = submit.upgrade() {
                        save.set_label("Save bookmark");
                    }
                }
                Err(error) => {
                    label.add_css_class("error");
                    label.set_text(&error);
                }
            }
        }
    });
    root
}

pub fn bookmark(
    menu: &crate::menu_sheet::MenuSheet,
    uri: &str,
    title: &str,
    saved: bool,
    private: bool,
    save: SaveBookmark,
    remove: Operation,
) {
    let root = panel();
    root.append(&site(uri));
    root.append(&bookmark_form(uri, title, saved, private, save, remove));
    menu.show_page("Bookmark this page", &root, ());
}
