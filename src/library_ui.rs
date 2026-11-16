//! Searchable native profile library. Page titles/URLs are displayed as text.
use gtk::{glib, prelude::*};
use paisley::library::Library;
use std::rc::Rc;

pub enum Change {
    ClearHistory,
    SaveBookmark { uri: String, title: String },
    RemoveBookmark(String),
    ImportBookmarks(paisley::bookmark_transfer::BookmarkImport),
}
pub type ReadLibrary = Rc<dyn Fn() -> Library>;
pub type OpenPage = Rc<dyn Fn(&str)>;
pub type UpdateLibrary = Rc<dyn Fn(Change) -> Result<(), String>>;

#[derive(Clone)]
struct Handles {
    window: glib::WeakRef<gtk::Window>,
    panel: Option<glib::WeakRef<gtk::Widget>>,
    query: glib::WeakRef<gtk::SearchEntry>,
    list: glib::WeakRef<gtk::Box>,
    status: glib::WeakRef<gtk::Label>,
    error: glib::WeakRef<gtk::Label>,
    bookmarks: bool,
    private: bool,
    icons: Option<Rc<crate::favicons::Favicons>>,
    lifetime: gtk::gio::Cancellable,
}
#[derive(Clone)]
struct Callbacks {
    read: ReadLibrary,
    open: OpenPage,
    update: UpdateLibrary,
}

#[derive(Clone)]
pub(crate) struct Manager {
    handles: Handles,
    callbacks: Callbacks,
}

impl Handles {
    fn live_window(&self) -> Option<gtk::Window> {
        if self.lifetime.is_cancelled() {
            return None;
        }
        self.window.upgrade().filter(|window| {
            window.is_visible()
                && if let Some(panel) = &self.panel {
                    panel
                        .upgrade()
                        .is_some_and(|panel| panel.is_mapped() && panel.parent().is_some())
                } else {
                    window
                        .transient_for()
                        .is_some_and(|owner| owner.is_visible())
                }
        })
    }
}

impl Manager {
    fn live_window(&self) -> Result<gtk::Window, String> {
        self.handles
            .live_window()
            .filter(|_| self.handles.bookmarks)
            .ok_or("Bookmark manager has closed".into())
    }

    fn message(&self, text: &str) {
        if let Some(error) = self.handles.error.upgrade() {
            error.set_text(text);
        }
    }

    pub(crate) fn import_file(&self, path: &std::path::Path) -> Result<(), String> {
        self.live_window()?;
        let import = paisley::bookmark_transfer::load_file(path)?;
        // Preview validation only; the Browser revalidates the current library
        // again at confirmation time instead of replacing a stale snapshot.
        paisley::bookmark_transfer::merge_import(&(self.callbacks.read)(), &import)?;
        let detail = format!(
            "The file contains {} bookmarks. Add new credential-free HTTP(S) URLs and plain-text titles without opening pages. Existing URLs keep their current bookmarks; history is unchanged. {}",
            import.count(),
            if self.handles.private {
                "Imported private bookmarks remain temporary and disappear on close."
            } else {
                "New bookmarks will be saved in this profile."
            }
        );
        confirm(
            &self.handles,
            &self.callbacks,
            Change::ImportBookmarks(import),
            "Import bookmarks?",
            &detail,
        );
        Ok(())
    }

    pub(crate) fn export_file(&self, path: &std::path::Path) -> Result<(), String> {
        self.live_window()?;
        paisley::bookmark_transfer::save_export(path, &(self.callbacks.read)())
    }

    fn choose_import(&self) {
        let Ok(window) = self.live_window() else {
            return;
        };
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Import Paisley bookmark JSON");
        let manager = self.clone();
        dialog.open(Some(&window), Some(&self.handles.lifetime), move |result| {
            if manager.live_window().is_err() {
                return;
            }
            match result {
                Ok(file) => match file
                    .path()
                    .ok_or("Select a local bookmark file".into())
                    .and_then(|path| manager.import_file(&path))
                {
                    Ok(()) => {
                        manager.message("Review the import confirmation; nothing has changed yet.")
                    }
                    Err(error) => manager.message(&error),
                },
                Err(error) => manager.chooser_error(error),
            }
        });
    }

    fn request_export(&self) {
        let Ok(window) = self.live_window() else {
            return;
        };
        let dialog = gtk::AlertDialog::builder().message("Export bookmark URLs and titles?")
            .detail("This explicitly saves potentially sensitive bookmark URLs and titles in an unencrypted Paisley JSON file. History and timestamps are omitted. Even in private mode, the exported file survives closing the window. Choose a new filename; existing files are never overwritten.")
            .buttons(["Cancel", "Choose export file…"]).cancel_button(0).default_button(0).modal(true).build();
        let manager = self.clone();
        dialog.choose(Some(&window), Some(&self.handles.lifetime), move |choice| {
            if choice != Ok(1) {
                return;
            }
            let Ok(window) = manager.live_window() else {
                return;
            };
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Export Paisley bookmark JSON");
            dialog.set_initial_name(Some("paisley-bookmarks.json"));
            let lifetime = manager.handles.lifetime.clone();
            dialog.save(Some(&window), Some(&lifetime), move |result| {
                if manager.live_window().is_err() {
                    return;
                }
                match result {
                    Ok(file) => match file
                        .path()
                        .ok_or("Select a local export destination".into())
                        .and_then(|path| manager.export_file(&path))
                    {
                        Ok(()) => manager.message(
                            "Bookmarks exported. The file contains URLs and titles, not history.",
                        ),
                        Err(error) => manager.message(&error),
                    },
                    Err(error) => manager.chooser_error(error),
                }
            });
        });
    }

    fn chooser_error(&self, error: glib::Error) {
        if !error.matches(gtk::gio::IOErrorEnum::Cancelled)
            && !error.matches(gtk::DialogError::Cancelled)
            && !error.matches(gtk::DialogError::Dismissed)
        {
            self.message(&format!("Bookmark file chooser failed: {error}"));
        }
    }
}

fn render(handles: &Handles, callbacks: &Callbacks) {
    let (Some(list), Some(query), Some(status)) = (
        handles.list.upgrade(),
        handles.query.upgrade(),
        handles.status.upgrade(),
    ) else {
        return;
    };
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let matches = (callbacks.read)().search(handles.bookmarks, query.text().as_str());
    status.set_text(&format!(
        "{} {}",
        matches.len(),
        if matches.len() == 1 {
            "result"
        } else {
            "results"
        }
    ));
    if matches.is_empty() {
        let empty = gtk::Label::builder()
            .label(if query.text().is_empty() {
                if handles.bookmarks {
                    "No bookmarks yet"
                } else {
                    "No browsing history yet"
                }
            } else {
                "No matching pages"
            })
            .wrap(true)
            .margin_top(32)
            .margin_bottom(32)
            .build();
        empty.add_css_class("dim-label");
        list.append(&empty);
    }
    for page in matches.into_iter().take(200) {
        let group = gtk::Box::new(gtk::Orientation::Vertical, 12);
        group.add_css_class("library-row");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let icon = if let Some(icons) = &handles.icons {
            icons.widget(&page.uri)
        } else {
            let icon = gtk::Image::from_icon_name("lucide-globe-symbolic");
            icon.set_pixel_size(18);
            icon.set_size_request(28, 28);
            icon.upcast::<gtk::Widget>()
        };
        let detail = gtk::Box::new(gtk::Orientation::Vertical, 5);
        let title = if page.title.is_empty() {
            &page.uri
        } else {
            &page.title
        };
        let label = gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .width_chars(1)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        label.add_css_class("downloads-filename");
        detail.append(&label);
        let domain = crate::download_ui::source(Some(&page.uri));
        let address = gtk::Label::builder()
            .label(&domain)
            .xalign(0.0)
            .width_chars(1)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .build();
        address.add_css_class("downloads-detail");
        detail.append(&address);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.append(&icon);
        content.append(&detail);
        detail.set_hexpand(true);
        let open = gtk::Button::builder().child(&content).hexpand(true).build();
        open.add_css_class("flat");
        open.set_tooltip_text(Some(&page.uri));
        let callback = Rc::clone(&callbacks.open);
        let uri = page.uri.clone();
        open.connect_clicked(move |_| callback(&uri));
        row.append(&open);
        group.append(&row);
        if handles.bookmarks {
            let options = gtk::ToggleButton::new();
            options.set_icon_name("lucide-ellipsis-symbolic");
            options.set_tooltip_text(Some("Edit bookmark"));
            options.set_valign(gtk::Align::Center);
            options.add_css_class("flat");
            row.append(&options);
            let h = handles.clone();
            let c = callbacks.clone();
            let uri = page.uri.clone();
            let save = Rc::new(move |title: &str| {
                h.live_window().ok_or("Bookmark manager has closed")?;
                (c.update)(Change::SaveBookmark {
                    uri: uri.clone(),
                    title: title.to_string(),
                })?;
                render(&h, &c);
                Ok(())
            });
            let h = handles.clone();
            let c = callbacks.clone();
            let uri = page.uri.clone();
            let remove = Rc::new(move || {
                h.live_window().ok_or("Bookmark manager has closed")?;
                (c.update)(Change::RemoveBookmark(uri.clone()))?;
                render(&h, &c);
                Ok(())
            });
            let editor = crate::browsing_ui::bookmark_form(
                &page.uri,
                &page.title,
                true,
                handles.private,
                save,
                remove,
            );
            editor.add_css_class("bookmark-editor");
            editor.set_visible(false);
            let weak = editor.downgrade();
            options.connect_toggled(move |button| {
                if let Some(editor) = weak.upgrade() {
                    editor.set_visible(button.is_active());
                }
            });
            group.append(&editor);
        }
        list.append(&group);
    }
}

fn confirm(handles: &Handles, callbacks: &Callbacks, change: Change, title: &str, detail: &str) {
    let Some(window) = handles.live_window() else {
        return;
    };
    let action = if matches!(&change, Change::ImportBookmarks(_)) {
        "Import"
    } else {
        "Remove"
    };
    let dialog = gtk::AlertDialog::builder()
        .message(title)
        .detail(detail)
        .buttons(["Cancel", action])
        .cancel_button(0)
        .default_button(0)
        .modal(true)
        .build();
    let handles = handles.clone();
    let callbacks = callbacks.clone();
    let lifetime = handles.lifetime.clone();
    dialog.choose(Some(&window), Some(&lifetime), move |choice| {
        if choice != Ok(1) || handles.live_window().is_none() {
            return;
        }
        match (callbacks.update)(change) {
            Ok(()) => {
                if let Some(error) = handles.error.upgrade() {
                    error.set_text("");
                }
                render(&handles, &callbacks);
            }
            Err(message) => {
                if let Some(error) = handles.error.upgrade() {
                    error.set_text(&message);
                }
            }
        }
    });
}

#[cfg(test)]
pub(crate) fn show(
    parent: &gtk::ApplicationWindow,
    bookmarks: bool,
    private: bool,
    read: ReadLibrary,
    open: OpenPage,
    update: UpdateLibrary,
) -> Manager {
    build(parent, bookmarks, private, read, open, update, None, None)
}

pub(crate) fn show_in_sheet(
    parent: &gtk::ApplicationWindow,
    bookmarks: bool,
    private: bool,
    read: ReadLibrary,
    open: OpenPage,
    update: UpdateLibrary,
    menu: &Rc<crate::menu_sheet::MenuSheet>,
    icons: Option<Rc<crate::favicons::Favicons>>,
) -> Manager {
    build(
        parent,
        bookmarks,
        private,
        read,
        open,
        update,
        Some(menu),
        icons,
    )
}

fn build(
    parent: &gtk::ApplicationWindow,
    bookmarks: bool,
    private: bool,
    read: ReadLibrary,
    open: OpenPage,
    update: UpdateLibrary,
    host: Option<&Rc<crate::menu_sheet::MenuSheet>>,
    icons: Option<Rc<crate::favicons::Favicons>>,
) -> Manager {
    let window: gtk::Window = if host.is_some() {
        parent.clone().upcast()
    } else {
        gtk::Window::builder()
            .title(if bookmarks {
                "Paisley bookmarks"
            } else {
                "Paisley history"
            })
            .transient_for(parent)
            .destroy_with_parent(true)
            .default_width(740)
            .default_height(580)
            .build()
    };
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let notice = if private {
        "Private window: history is not recorded; bookmarks here are temporary and disappear on close."
    } else {
        "Profile-local URLs and page titles may be sensitive and are not encrypted. History is bounded to 1,000 unique URLs and bookmarks to 256; old history may be evicted earlier to fit the file-size limit."
    };
    let info = gtk::Label::builder()
        .label(if private {
            "Private · temporary bookmarks"
        } else {
            "Saved in this profile"
        })
        .xalign(0.0)
        .build();
    info.set_tooltip_text(Some(notice));
    info.add_css_class("downloads-detail");
    root.append(&info);
    let query = gtk::SearchEntry::builder()
        .placeholder_text("Search titles or URLs")
        .hexpand(true)
        .build();
    root.append(&query);
    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let refresh = gtk::Button::from_icon_name("lucide-refresh-cw-symbolic");
    refresh.set_tooltip_text(Some("Refresh saved pages"));
    tools.append(&refresh);
    let clear = gtk::Button::with_label("Clear history…");
    if !bookmarks {
        tools.append(&clear);
    }
    let import = gtk::Button::with_label("Import…");
    let export = gtk::Button::with_label("Export…");
    if bookmarks {
        tools.append(&import);
        tools.append(&export);
    }
    for button in [&refresh, &clear, &import, &export] {
        button.add_css_class("flat");
    }
    root.append(&tools);
    let status = gtk::Label::builder().xalign(0.0).build();
    root.append(&status);
    let error = gtk::Label::builder().wrap(true).xalign(0.0).build();
    crate::download_ui::hide_when_empty(&error);
    error.add_css_class("error");
    root.append(&error);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list)
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    root.append(&scroller);
    if host.is_none() {
        window.set_child(Some(&root));
    } else {
        root.set_margin_start(0);
        root.set_margin_end(0);
        root.set_margin_top(0);
        root.set_margin_bottom(0);
    }
    let handles = Handles {
        window: window.downgrade(),
        panel: host.map(|_| root.clone().upcast::<gtk::Widget>().downgrade()),
        query: query.downgrade(),
        list: list.downgrade(),
        status: status.downgrade(),
        error: error.downgrade(),
        bookmarks,
        private,
        icons,
        lifetime: gtk::gio::Cancellable::new(),
    };
    if host.is_none() {
        let lifetime = handles.lifetime.clone();
        window.connect_close_request(move |_| {
            lifetime.cancel();
            glib::Propagation::Proceed
        });
        let lifetime = handles.lifetime.downgrade();
        let owner_close = parent.connect_close_request(move |_| {
            if let Some(lifetime) = lifetime.upgrade() {
                lifetime.cancel();
            }
            glib::Propagation::Proceed
        });
        let owner = parent.downgrade();
        let owner_close = std::cell::RefCell::new(Some(owner_close));
        let lifetime = handles.lifetime.clone();
        window.connect_destroy(move |_| {
            lifetime.cancel();
            if let Some(owner) = owner.upgrade()
                && let Some(handler) = owner_close.borrow_mut().take()
            {
                owner.disconnect(handler);
            }
        });
    } else {
        let lifetime = handles.lifetime.clone();
        root.connect_destroy(move |_| lifetime.cancel());
    }
    let callbacks = Callbacks { read, open, update };
    let manager = Manager {
        handles: handles.clone(),
        callbacks: callbacks.clone(),
    };
    let m = manager.clone();
    import.connect_clicked(move |_| m.choose_import());
    let m = manager.clone();
    export.connect_clicked(move |_| m.request_export());
    let h = handles.clone();
    let c = callbacks.clone();
    query.connect_search_changed(move |_| render(&h, &c));
    let h = handles.clone();
    let c = callbacks.clone();
    refresh.connect_clicked(move |_| render(&h, &c));
    let h = handles.clone();
    let c = callbacks.clone();
    clear.connect_clicked(move |_| confirm(&h, &c, Change::ClearHistory, "Clear this profile's history?", "Remove recorded URLs and page titles. Bookmarks, cookies, website data and restored tabs are not cleared."));
    render(&handles, &callbacks);
    if let Some(host) = host {
        host.show_page(
            if bookmarks { "Bookmarks" } else { "History" },
            &root,
            manager.clone(),
        );
    } else {
        window.present();
    }
    manager
}
