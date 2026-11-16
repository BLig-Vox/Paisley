//! Native address-bar site information. Security claims come from WebKit.
use gtk::prelude::*;
use std::rc::Rc;

pub struct SiteInfo {
    pub origin: String,
    pub connection: String,
    pub certificate: String,
    pub supported: bool,
    pub enabled: bool,
    pub available: bool,
    pub global: bool,
}
fn detail(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .max_width_chars(36)
        .xalign(0.0)
        .build()
}
fn action_button(text: &str, icon: &str) -> gtk::Button {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(18);
    row.append(&image);
    row.append(&crate::download_ui::label(text, "site-action-label"));
    let arrow = gtk::Image::from_icon_name("lucide-chevron-right-symbolic");
    arrow.set_pixel_size(14);
    row.append(&arrow);
    gtk::Button::builder().child(&row).build()
}
pub fn content(
    state: SiteInfo,
    toggle: Rc<dyn Fn(bool) -> Result<(), String>>,
    clear: Rc<dyn Fn()>,
    popover: &gtk::Popover,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 14);
    root.set_width_request(350);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let origin = detail(&state.origin);
    origin.set_selectable(true);
    origin.add_css_class("notification-title");
    root.append(&origin);
    let connection = gtk::Expander::new(Some(&state.connection));
    let cert = detail(&state.certificate);
    cert.set_margin_top(10);
    cert.add_css_class("downloads-detail");
    cert.set_selectable(true);
    connection.set_child(Some(&cert));
    root.append(&connection);
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("site-protection-card");
    let icon = gtk::Image::from_icon_name(if state.enabled && state.available && state.supported {
        "lucide-shield-check-symbolic"
    } else {
        "lucide-shield-symbolic"
    });
    icon.set_pixel_size(38);
    card.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 5);
    text.set_hexpand(true);
    let title = detail(if state.enabled && state.available && state.supported {
        "Paisley is on guard"
    } else {
        "Site protection"
    });
    title.add_css_class("notification-title");
    text.append(&title);
    text.append(&detail(if !state.supported {
        "Protection is available on websites."
    } else if !state.global {
        "Blocking is off in Settings."
    } else if !state.available {
        "Protection is preparing. This site has not been declared protected."
    } else if state.enabled {
        "Starter blocking rules are enabled for this site."
    } else {
        "Blocking is off for this site."
    }));
    card.append(&text);
    root.append(&card);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.append(&crate::download_ui::label(
        "Block ads & trackers",
        "notification-title",
    ));
    let switch = gtk::Switch::new();
    switch.set_valign(gtk::Align::Center);
    switch.set_active(state.enabled && state.supported);
    switch.set_sensitive(state.supported && state.available && state.global);
    row.append(&switch);
    root.append(&row);
    let note = detail("Reload the page after changing protection.");
    note.add_css_class("downloads-detail");
    root.append(&note);
    let feedback = detail("");
    feedback.add_css_class("downloads-detail");
    crate::download_ui::hide_when_empty(&feedback);
    root.append(&feedback);
    let weak = feedback.downgrade();
    let popup = popover.downgrade();
    let resetting = std::cell::Cell::new(false);
    switch.connect_state_set(move |switch, on| {
        if resetting.get() {
            return gtk::glib::Propagation::Stop;
        }
        match toggle(on) {
            Ok(()) => {
                switch.set_state(on);
                if let Some(popup) = popup.upgrade() {
                    popup.popdown();
                }
            }
            Err(error) => {
                resetting.set(true);
                switch.set_active(switch.state());
                resetting.set(false);
                if let Some(label) = weak.upgrade() {
                    label.set_text(&error);
                    label.add_css_class("error");
                }
            }
        }
        gtk::glib::Propagation::Stop
    });
    root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let clear_button = action_button("Clear cookies and site data…", "lucide-trash-symbolic");
    clear_button.add_css_class("flat");
    clear_button.set_sensitive(state.supported);
    let popup = popover.downgrade();
    clear_button.connect_clicked(move |_| {
        if let Some(popup) = popup.upgrade() {
            popup.popdown();
        }
        clear();
    });
    root.append(&clear_button);
    for (label, action) in [
        ("Notification permissions", "win.notifications"),
        ("Privacy settings", "win.settings"),
    ] {
        let button = action_button(
            label,
            if action == "win.notifications" {
                "lucide-bell-symbolic"
            } else {
                "lucide-settings-symbolic"
            },
        );
        button.add_css_class("flat");
        button.set_action_name(Some(action));
        let popup = popover.downgrade();
        button.connect_clicked(move |_| {
            if let Some(popup) = popup.upgrade() {
                popup.popdown();
            }
        });
        root.append(&button);
    }
    root
}

pub async fn clear_site_data(
    parent: &gtk::ApplicationWindow,
    manager: &webkit::WebsiteDataManager,
    host: &str,
) -> Result<String, String> {
    let types = webkit::WebsiteDataTypes::COOKIES
        | webkit::WebsiteDataTypes::LOCAL_STORAGE
        | webkit::WebsiteDataTypes::SESSION_STORAGE
        | webkit::WebsiteDataTypes::INDEXEDDB_DATABASES
        | webkit::WebsiteDataTypes::DOM_CACHE
        | webkit::WebsiteDataTypes::SERVICE_WORKER_REGISTRATIONS
        | webkit::WebsiteDataTypes::DISK_CACHE
        | webkit::WebsiteDataTypes::MEMORY_CACHE;
    let records = manager
        .fetch_future(types)
        .await
        .map_err(|e| format!("Could not inspect site data: {e}"))?;
    let selected: Vec<_> = records
        .into_iter()
        .filter(|record| {
            record.name().is_some_and(|name| {
                let name = name.to_ascii_lowercase();
                !name.is_empty() && (name == host || host.ends_with(&format!(".{name}")))
            })
        })
        .collect();
    if selected.is_empty() {
        return Ok(format!(
            "No stored cookies or site data were found for {host}."
        ));
    }
    if selected.len() > 32 {
        return Err("Too many matching site records; no data was cleared.".into());
    }
    if !parent.is_visible() {
        return Err("The browser window closed; no data was cleared.".into());
    }
    let mut names: Vec<_> = selected
        .iter()
        .filter_map(|r| r.name())
        .map(|s| s.to_string())
        .collect();
    names.sort();
    names.dedup();
    let confirm=gtk::AlertDialog::builder().message(format!("Clear data for {host}?"))
        .detail(format!("WebKit groups stored data under these names:\n\n{}\n\nCookies, storage, caches and service workers in these records will be removed in this profile. This may include related subdomains and sign you out. Other open tabs may recreate data. Bookmarks, history, downloads and notification permissions stay unchanged.",names.join("\n")))
        .buttons(["Cancel","Clear site data"]).cancel_button(0).default_button(0).modal(true).build();
    if confirm.choose_future(Some(parent)).await.ok() != Some(1) {
        return Ok("Site data clearing cancelled.".into());
    }
    gtk::gio::GioFuture::new(manager, move |obj, cancellable, send| {
        obj.remove(
            types,
            &selected.iter().collect::<Vec<_>>(),
            Some(cancellable),
            move |result| {
                send.resolve(result);
            },
        );
    })
    .await
    .map_err(|e| format!("Could not clear site data: {e}"))?;
    Ok(format!(
        "Site data cleared for {}. Reload the site; other open tabs may recreate data.",
        names.join(", ")
    ))
}
