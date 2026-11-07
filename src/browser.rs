use std::cell::{Cell, RefCell};
use std::rc::Rc;

use browny::navigation;
use gtk::prelude::*;
use webkit::prelude::*;

#[derive(Clone)]
struct Tab {
    id: u64,
    page: gtk::Box,
    view: webkit::WebView,
    title: gtk::Label,
    spinner: gtk::Spinner,
    error: gtk::Label,
    placeholder: gtk::Label,
    generation: Rc<Cell<u64>>,
    committed: Rc<Cell<bool>>,
}

pub struct Browser {
    window: gtk::ApplicationWindow,
    notebook: gtk::Notebook,
    address: gtk::Entry,
    back: gtk::Button,
    forward: gtk::Button,
    reload: gtk::Button,
    progress: gtk::ProgressBar,
    session: webkit::NetworkSession,
    tabs: RefCell<Vec<Tab>>,
    next_id: Cell<u64>,
    memory: RefCell<Option<browny::memory::Usage>>,
    profile: std::sync::Arc<browny::profile::Profile>,
    profile_label: String,
    settings: RefCell<browny::profile::Settings>,
    library: RefCell<browny::library::Library>,
    pending: RefCell<std::collections::HashMap<u64, String>>,
    dirty: Cell<bool>,
    closing: Cell<bool>,
    writer: RefCell<Option<crate::session_store::SessionWriter>>,
    downloads: RefCell<Rc<crate::downloads::Downloads>>,
    blocker: Rc<crate::blocking::Blocker>,
    status: gtk::Label,
}

impl Browser {
    #[cfg(test)]
    pub fn new(app: &gtk::Application) -> Rc<Self> {
        Self::with_profile(
            app,
            browny::profile::Profile::open("test-private", true).unwrap(),
        )
        .unwrap()
    }

    pub fn with_profile(
        app: &gtk::Application,
        profile: browny::profile::Profile,
    ) -> Result<Rc<Self>, String> {
        let restored = if profile.settings.restore_session {
            profile.load_session()?
        } else {
            browny::profile::Session::default()
        };
        let library = profile.load_library()?;
        // Nonessential metadata must not make the whole profile unusable.
        // Preserve the unreadable/future record and disable only these writes.
        let (download_history, download_error) = match profile.load_download_history() {
            Ok(history) => (history, None),
            Err(error) => (Default::default(), Some(error)),
        };
        let settings = profile.settings.clone();
        let profile = std::sync::Arc::new(profile);
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Browny")
            .default_width(1280)
            .default_height(800)
            .width_request(640)
            .height_request(400)
            .build();

        let address = gtk::Entry::builder()
            .placeholder_text("Search or enter address")
            .hexpand(true)
            .width_chars(40)
            .tooltip_text("Address or DuckDuckGo search (Ctrl+L)")
            .build();
        let back = icon_button("go-previous-symbolic", "Back (Alt+Left)", "win.back");
        let forward = icon_button("go-next-symbolic", "Forward (Alt+Right)", "win.forward");
        let reload = icon_button("view-refresh-symbolic", "Reload (Ctrl+R)", "win.reload");
        let home = icon_button("go-home-symbolic", "Home", "win.home");
        let new_tab = icon_button("tab-new-symbolic", "New tab (Ctrl+T)", "win.new-tab");
        let header = gtk::HeaderBar::new();
        header.pack_start(&back);
        header.pack_start(&forward);
        header.pack_start(&reload);
        header.pack_start(&home);
        header.set_title_widget(Some(&address));
        header.pack_end(&new_tab);
        header.pack_end(&icon_button(
            "folder-download-symbolic",
            "Downloads (Ctrl+J)",
            "win.downloads",
        ));
        header.pack_end(&icon_button(
            "applications-engineering-symbolic",
            "Web Inspector / Console (F12)",
            "win.inspector",
        ));
        let menu = gtk::gio::Menu::new();
        menu.append(Some("Settings"), Some("win.settings"));
        menu.append(Some("Profiles / private window"), Some("win.profiles"));
        menu.append(
            Some("Toggle blocking for this site"),
            Some("win.site-blocking"),
        );
        menu.append(
            Some("Bookmark this page (Ctrl+D)"),
            Some("win.bookmark-current"),
        );
        menu.append(Some("Bookmarks"), Some("win.bookmarks"));
        menu.append(Some("History (Ctrl+H)"), Some("win.history"));
        menu.append(Some("Downloads"), Some("win.downloads"));
        menu.append(Some("Web Inspector / Console"), Some("win.inspector"));
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .tooltip_text("Browny menu")
            .build();
        header.pack_end(&menu_button);
        window.set_titlebar(Some(&header));

        let notebook = gtk::Notebook::builder()
            .scrollable(true)
            .show_border(false)
            .hexpand(true)
            .vexpand(true)
            .build();
        let progress = gtk::ProgressBar::builder()
            .visible(false)
            .show_text(false)
            .build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&progress);
        root.append(&notebook);
        let status = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .margin_start(8)
            .margin_end(8)
            .build();
        root.append(&status);
        window.set_child(Some(&root));

        let session = if profile.private {
            webkit::NetworkSession::new_ephemeral()
        } else {
            let data = profile.data_dir.join("webkit");
            let session = webkit::NetworkSession::new(
                Some(data.to_str().ok_or("Profile data path is not UTF-8")?),
                Some(
                    profile
                        .cache_dir
                        .to_str()
                        .ok_or("Profile cache path is not UTF-8")?,
                ),
            );
            if let Some(cookies) = session.cookie_manager() {
                let path = profile.data_dir.join("cookies.sqlite");
                cookies.set_persistent_storage(
                    path.to_str().ok_or("Cookie path is not UTF-8")?,
                    webkit::CookiePersistentStorage::Sqlite,
                );
            }
            session
        };
        session.set_itp_enabled(true);
        if settings.strict_privacy
            && let Some(cookies) = session.cookie_manager()
        {
            cookies.set_accept_policy(webkit::CookieAcceptPolicy::NoThirdParty);
        }
        let writer = crate::session_store::SessionWriter::new(std::sync::Arc::clone(&profile));
        let downloads =
            crate::downloads::Downloads::for_profile(&window, download_history, profile.private);
        if let Some(error) = download_error {
            downloads.use_memory_records(&error);
        }
        let blocker = crate::blocking::Blocker::new(&profile)?;
        let profile_label = match profile.display_name() {
            Ok(name) if name != profile.id => format!("{name} [{}]", profile.id),
            Ok(_) => profile.id.clone(),
            Err(_) => format!("{} (display name unavailable)", profile.id),
        };
        let browser = Rc::new(Self {
            window,
            notebook,
            address,
            back,
            forward,
            reload,
            progress,
            session,
            tabs: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
            memory: RefCell::new(None),
            profile,
            profile_label,
            settings: RefCell::new(settings),
            library: RefCell::new(library),
            pending: RefCell::new(std::collections::HashMap::new()),
            dirty: Cell::new(false),
            closing: Cell::new(false),
            writer: RefCell::new(Some(writer)),
            downloads: RefCell::new(downloads),
            blocker,
            status,
        });
        browser.install_actions(app);
        browser.track_download_changes();
        browser.connect_window();
        let weak = Rc::downgrade(&browser);
        browser
            .blocker
            .compile(crate::blocking::STARTER_RULES, move || {
                if let Some(browser) = weak.upgrade() {
                    for tab in browser.tabs.borrow().iter() {
                        browser.apply_site_blocking(tab, tab.view.uri().as_deref());
                    }
                    if let Some(tab) = browser.active_tab() {
                        browser.activate_pending(&tab);
                    }
                    browser.sync_active(false);
                }
            });
        if restored.tabs.is_empty() {
            browser.add_tab(None, Some("browny:home"));
        } else {
            for uri in &restored.tabs {
                browser.add_tab(None, None);
                if let Some(tab) = browser.active_tab() {
                    browser.pending.borrow_mut().insert(tab.id, uri.clone());
                }
            }
            browser
                .notebook
                .set_current_page(Some(restored.active.min(restored.tabs.len() - 1) as u32));
            if let Some(tab) = browser.active_tab() {
                browser.activate_pending(&tab);
            }
        }
        browser.start_memory_monitor();
        browser.start_session_writer();
        Ok(browser)
    }

    pub fn present(self: &Rc<Self>) {
        self.window.present();
        self.focus_address();
    }

    fn snapshot_session(&self) -> browny::profile::Session {
        if !self.settings.borrow().restore_session {
            return browny::profile::Session::default();
        }
        let mut tabs = Vec::new();
        for index in 0..self.notebook.n_pages() {
            if let Some(page) = self.notebook.nth_page(Some(index))
                && let Some(tab) = self.tabs.borrow().iter().find(|tab| tab.page == page)
            {
                if tab.placeholder.is_visible() {
                    tabs.push(browny::profile::NON_RESTORABLE_TAB.to_owned());
                    continue;
                }
                let uri = self
                    .pending
                    .borrow()
                    .get(&tab.id)
                    .cloned()
                    .or_else(|| tab.view.uri().map(|s| s.to_string()))
                    .unwrap_or_else(|| self.settings.borrow().homepage.clone());
                // Normalize before background queueing, not only at disk write.
                tabs.push(
                    browny::profile::session_target(&uri)
                        .unwrap_or("")
                        .to_owned(),
                );
            }
        }
        browny::profile::Session {
            tabs,
            active: self.notebook.current_page().unwrap_or(0) as usize,
        }
    }

    fn apply_settings(&self) {
        let strict = self.settings.borrow().strict_privacy;
        if let Some(cookies) = self.session.cookie_manager() {
            cookies.set_accept_policy(if strict {
                webkit::CookieAcceptPolicy::NoThirdParty
            } else {
                webkit::CookieAcceptPolicy::Always
            });
        }
        for tab in self.tabs.borrow().iter() {
            if let Some(settings) = webkit::prelude::WebViewExt::settings(&tab.view) {
                settings.set_enable_webrtc(!strict);
            }
            self.apply_site_blocking(tab, tab.view.uri().as_deref());
        }
        if let Some(tab) = self.active_tab() {
            self.activate_pending(&tab);
        }
        self.dirty.set(true);
        self.sync_active(false);
    }

    fn blocking_enabled(&self, uri: Option<&str>) -> bool {
        let settings = self.settings.borrow();
        settings.block_trackers
            && !uri
                .and_then(origin)
                .is_some_and(|origin| settings.blocking_exceptions.contains(&origin))
    }

    fn apply_site_blocking(&self, tab: &Tab, uri: Option<&str>) {
        if let Some(manager) = tab.view.user_content_manager()
            && let Some(filter) = self.blocker.filter()
        {
            manager.remove_filter(&filter);
            if self.blocking_enabled(uri) {
                manager.add_filter(&filter);
            }
        }
    }

    fn activate_pending(&self, tab: &Tab) {
        let pending = self.pending.borrow().get(&tab.id).cloned();
        if let Some(uri) = pending {
            self.load_tab(tab, &uri);
        }
    }

    fn load_tab(&self, tab: &Tab, uri: &str) {
        if uri == browny::profile::NON_RESTORABLE_TAB {
            // Metadata sentinel only: never send it or the original transient
            // URL/payload through WebKit, search, network or script execution.
            self.pending.borrow_mut().remove(&tab.id);
            tab.placeholder.set_visible(true);
            tab.view.set_visible(false);
            tab.view.stop_loading();
            tab.committed.set(false);
            tab.error.set_visible(false);
            tab.title.set_text("Not restored");
            tab.title.set_tooltip_text(None);
            tab.spinner.set_spinning(false);
            tab.spinner.set_visible(false);
            self.dirty.set(true);
            return;
        }
        tab.placeholder.set_visible(false);
        tab.view.set_visible(true);
        if self.blocking_enabled(Some(uri))
            && (!self.blocker.ready.get() || self.blocker.filter().is_none())
        {
            self.pending.borrow_mut().insert(tab.id, uri.to_string());
            if let Some(error) = self.blocker.error.borrow().as_ref() {
                show_error(
                    tab,
                    &format!(
                        "Protection could not start: {error}. Turn blocking off in Settings to continue without it."
                    ),
                );
            }
            return;
        }
        self.pending.borrow_mut().remove(&tab.id);
        self.apply_site_blocking(tab, Some(uri));
        tab.error.set_visible(false);
        tab.view.load_uri(uri);
    }

    fn start_session_writer(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
            let Some(browser) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if browser.closing.get() {
                return gtk::glib::ControlFlow::Break;
            }
            if let Some(writer) = browser.writer.borrow().as_ref() {
                if browser.dirty.get()
                    && writer.submit(
                        browser.snapshot_session(),
                        browser.library.borrow().clone(),
                        browser.downloads.borrow().storage_snapshot(),
                    )
                {
                    browser.dirty.set(false);
                }
                if let Some(error) = writer.error() {
                    // Queue acceptance is not persistence. Keep the newest state
                    // pending after a failed write, even without another edit.
                    browser.dirty.set(true);
                    if let Some(tab) = browser.active_tab() {
                        show_error(&tab, &format!("Could not save browser state: {error}"));
                    }
                }
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    fn start_memory_monitor(self: &Rc<Self>) {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            loop {
                if send
                    .send(browny::memory::sample(std::process::id()))
                    .is_err()
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
        });
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local(std::time::Duration::from_secs(2), move || {
            let Some(browser) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if let Ok(Some(usage)) = receive.try_recv() {
                *browser.memory.borrow_mut() = Some(usage);
                browser.sync_active(false);
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    fn track_download_changes(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.downloads.borrow().on_changed(move || {
            if let Some(browser) = weak.upgrade() {
                browser.dirty.set(true);
            }
        });
    }

    fn connect_window(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.address.connect_activate(move |entry| {
            if let Some(browser) = weak.upgrade() {
                browser.navigate(entry.text().as_str());
            }
        });

        let weak = Rc::downgrade(self);
        self.notebook.connect_switch_page(move |_, page, _| {
            if let Some(browser) = weak.upgrade() {
                // During switch-page emission current_page() may still refer
                // to the previous page; use the signal's actual target.
                let tab = browser
                    .tabs
                    .borrow()
                    .iter()
                    .find(|tab| tab.page == *page)
                    .cloned();
                if let Some(tab) = tab {
                    browser.activate_pending(&tab);
                    browser.sync_controls(&tab, true);
                    browser.dirty.set(true);
                }
            }
        });

        let weak = Rc::downgrade(self);
        self.notebook.connect_page_reordered(move |_, _, _| {
            if let Some(browser) = weak.upgrade() {
                browser.dirty.set(true);
            }
        });

        let weak = Rc::downgrade(self);
        self.session.connect_download_started(move |_, download| {
            if let Some(browser) = weak.upgrade() {
                let downloads = browser.downloads.borrow().clone();
                downloads.show();
                downloads.start(download);
            } else {
                download.cancel();
            }
        });

        // Keep the controller alive while its window exists. Every other
        // callback holds only a Weak reference. Release this sole signal-owned
        // strong reference on close, breaking the window/controller cycle.
        let owner = RefCell::new(Some(Rc::clone(self)));
        self.window.connect_close_request(move |_| {
            if let Some(browser) = owner.borrow_mut().take() {
                browser.closing.set(true);
                browser.downloads.borrow().shutdown();
                let snapshot = browser.snapshot_session();
                if let Some(writer) = browser.writer.borrow_mut().take()
                    && let Err(error) = writer.finish(
                        snapshot,
                        browser.library.borrow().clone(),
                        browser.downloads.borrow().storage_snapshot(),
                    )
                {
                    eprintln!("Could not save final session: {error}");
                }
                let tabs = browser.tabs.replace(Vec::new());
                for tab in tabs {
                    if let Some(inspector) = tab.view.inspector() {
                        inspector.close();
                    }
                    tab.view.stop_loading();
                }
            }
            gtk::glib::Propagation::Proceed
        });
    }

    fn show_library(self: &Rc<Self>, bookmarks: bool) -> crate::library_ui::Manager {
        let weak = Rc::downgrade(self);
        let read: crate::library_ui::ReadLibrary = Rc::new(move || {
            weak.upgrade()
                .map(|b| b.library.borrow().clone())
                .unwrap_or_default()
        });
        let weak = Rc::downgrade(self);
        let open: crate::library_ui::OpenPage = Rc::new(move |uri| {
            if let Some(browser) = weak.upgrade() {
                browser.navigate(uri);
            }
        });
        let weak = Rc::downgrade(self);
        let update: crate::library_ui::UpdateLibrary = Rc::new(move |change| {
            let browser = weak.upgrade().ok_or("Browser has closed")?;
            match change {
                crate::library_ui::Change::ClearHistory => {
                    browser.library.borrow_mut().history.clear()
                }
                crate::library_ui::Change::RemoveBookmark(uri) => {
                    browser.library.borrow_mut().remove_bookmark(&uri)
                }
                crate::library_ui::Change::ImportBookmarks(import) => {
                    let next = browny::bookmark_transfer::merge_import(
                        &browser.library.borrow(),
                        &import,
                    )?;
                    *browser.library.borrow_mut() = next;
                }
            }
            browser.dirty.set(true);
            Ok(())
        });
        crate::library_ui::show(
            &self.window,
            bookmarks,
            self.profile.private,
            read,
            open,
            update,
        )
    }

    fn install_actions(self: &Rc<Self>, app: &gtk::Application) {
        self.action("bookmark-current", |browser| {
            if let Some(tab) = browser.active_tab()
                && let Some(uri) = tab.view.uri()
            {
                let title = tab.view.title().unwrap_or_default();
                let result = browser.library.borrow_mut().bookmark(&uri, &title, now());
                match result {
                    Ok(()) => {
                        browser.dirty.set(true);
                        browser.status.set_text(if browser.profile.private {
                            "Temporary private bookmark saved"
                        } else {
                            "Bookmark saved in this profile"
                        });
                    }
                    Err(error) => show_error(&tab, &error),
                }
            }
        });
        self.action("bookmarks", |browser| {
            browser.show_library(true);
        });
        self.action("history", |browser| {
            browser.show_library(false);
        });
        self.action("settings", |browser| {
            let weak = Rc::downgrade(browser);
            crate::settings_ui::show(
                &browser.window,
                browser.settings.borrow().clone(),
                move |settings| {
                    let browser = weak.upgrade().ok_or("Browser has closed")?;
                    browser.profile.save_settings(&settings)?;
                    *browser.settings.borrow_mut() = settings;
                    browser.apply_settings();
                    Ok(())
                },
            );
        });
        self.action("profiles", |browser| {
            crate::settings_ui::profiles(
                &browser.window,
                &browser.profile.id,
                browser.profile.private,
            )
        });
        self.action("site-blocking", |browser| {
            if let Some(tab) = browser.active_tab()
                && let Some(uri) = tab.view.uri()
                && let Some(origin) = origin(&uri)
            {
                let mut settings = browser.settings.borrow().clone();
                if settings.blocking_exceptions.contains(&origin) {
                    settings.blocking_exceptions.retain(|item| item != &origin);
                } else {
                    settings.blocking_exceptions.push(origin);
                }
                match browser.profile.save_settings(&settings) {
                    Ok(()) => {
                        *browser.settings.borrow_mut() = settings;
                        browser.apply_settings();
                    }
                    Err(error) => {
                        show_error(&tab, &format!("Could not save site exception: {error}"))
                    }
                }
            }
        });
        self.action("new-tab", |browser| {
            browser.add_tab(None, Some("browny:home"));
            browser.focus_address();
        });
        self.action("close-tab", |browser| {
            if let Some(tab) = browser.active_tab() {
                browser.close_tab(tab.id);
            }
        });
        self.action("downloads", |browser| browser.downloads.borrow().show());
        self.action("inspector", |browser| {
            if let Some(tab) = browser.active_tab()
                && let Some(inspector) = tab.view.inspector()
            {
                if inspector.is_attached()
                    || inspector.web_view().is_some_and(|view| view.is_visible())
                {
                    inspector.close();
                } else {
                    inspector.show();
                }
            }
        });
        self.action("focus-address", |browser| browser.focus_address());
        self.action("next-tab", |browser| browser.cycle_tab(1));
        self.action("previous-tab", |browser| browser.cycle_tab(-1));
        self.action("home", |browser| browser.navigate("browny:home"));
        self.action("back", |browser| {
            if let Some(tab) = browser.active_tab()
                && tab.view.can_go_back()
            {
                tab.view.go_back();
            }
        });
        self.action("forward", |browser| {
            if let Some(tab) = browser.active_tab()
                && tab.view.can_go_forward()
            {
                tab.view.go_forward();
            }
        });
        self.action("reload", |browser| {
            if let Some(tab) = browser.active_tab()
                && !tab.placeholder.is_visible()
            {
                tab.error.set_visible(false);
                tab.view.reload();
            }
        });
        self.action("reload-or-stop", |browser| {
            if let Some(tab) = browser.active_tab()
                && !tab.placeholder.is_visible()
            {
                if tab.view.is_loading() {
                    tab.view.stop_loading();
                } else {
                    tab.error.set_visible(false);
                    tab.view.reload();
                }
            }
        });
        self.reload.set_action_name(Some("win.reload-or-stop"));
        self.action("stop", |browser| {
            if let Some(tab) = browser.active_tab() {
                tab.view.stop_loading();
                tab.view.grab_focus();
            }
        });

        for (action, shortcuts) in [
            ("win.new-tab", &["<Control>t"][..]),
            ("win.close-tab", &["<Control>w"][..]),
            ("win.focus-address", &["<Control>l"][..]),
            (
                "win.inspector",
                &["F12", "<Control><Shift>i", "<Control><Shift>j"][..],
            ),
            ("win.downloads", &["<Control>j"][..]),
            ("win.bookmark-current", &["<Control>d"][..]),
            ("win.history", &["<Control>h"][..]),
            ("win.next-tab", &["<Control>Tab", "<Control>Page_Down"][..]),
            (
                "win.previous-tab",
                &["<Control><Shift>Tab", "<Control>Page_Up"][..],
            ),
            ("win.reload", &["<Control>r", "F5"][..]),
            ("win.back", &["<Alt>Left"][..]),
            ("win.forward", &["<Alt>Right"][..]),
            ("win.stop", &["Escape"][..]),
        ] {
            app.set_accels_for_action(action, shortcuts);
        }
    }

    fn action(self: &Rc<Self>, name: &str, callback: impl Fn(&Rc<Self>) + 'static) {
        let action = gtk::gio::SimpleAction::new(name, None);
        let weak = Rc::downgrade(self);
        action.connect_activate(move |_, _| {
            if let Some(browser) = weak.upgrade() {
                callback(&browser);
            }
        });
        self.window.add_action(&action);
    }

    fn focus_address(&self) {
        self.address.grab_focus();
        self.address.select_region(0, -1);
    }

    fn cycle_tab(&self, direction: i32) {
        if let Some(index) = self.notebook.current_page() {
            let count = self.notebook.n_pages() as i32;
            if count > 0 {
                self.notebook
                    .set_current_page(Some((index as i32 + direction).rem_euclid(count) as u32));
            }
        }
    }

    fn active_tab(&self) -> Option<Tab> {
        let page = self.notebook.nth_page(self.notebook.current_page())?;
        self.tabs
            .borrow()
            .iter()
            .find(|tab| tab.page == page)
            .cloned()
    }

    fn tab(&self, id: u64) -> Option<Tab> {
        self.tabs.borrow().iter().find(|tab| tab.id == id).cloned()
    }

    fn navigate(&self, input: &str) {
        if let Some(tab) = self.active_tab() {
            match navigation::resolve(input) {
                Ok(target) => {
                    let uri = if target == navigation::Navigation::Home {
                        self.settings.borrow().homepage.clone()
                    } else {
                        target.uri().to_string()
                    };
                    self.pending.borrow_mut().remove(&tab.id);
                    self.dirty.set(true);
                    tab.error.set_visible(false);
                    self.load_tab(&tab, &uri);
                    tab.view.grab_focus();
                    // Explicit navigation is not an in-progress address edit.
                    // Do not depend on a URI notification or focus transfer:
                    // an unmapped WebView cannot take focus immediately.
                    self.address.set_text(&uri);
                }
                Err(message) => show_error(&tab, &message),
            }
        }
    }

    fn add_tab(
        self: &Rc<Self>,
        related: Option<&webkit::WebView>,
        initial: Option<&str>,
    ) -> webkit::WebView {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        let manager = crate::site_styles::content_manager();
        // Page-created related views can navigate before load_tab is called.
        // Install protection on their content manager before returning the view.
        if self.settings.borrow().block_trackers
            && let Some(filter) = self.blocker.filter()
        {
            manager.add_filter(&filter);
        }
        let builder = webkit::WebView::builder().user_content_manager(&manager);
        let view = match related {
            Some(source) => builder.related_view(source).build(),
            None => builder.network_session(&self.session).build(),
        };
        if let Some(settings) = webkit::prelude::WebViewExt::settings(&view) {
            settings.set_enable_developer_extras(true);
            settings.set_media_playback_requires_user_gesture(true);
            settings.set_javascript_can_open_windows_automatically(false);
        }
        if self.settings.borrow().strict_privacy
            && let Some(settings) = webkit::prelude::WebViewExt::settings(&view)
        {
            settings.set_enable_webrtc(false);
        }
        view.set_hexpand(true);
        view.set_vexpand(true);
        let error = gtk::Label::builder()
            .visible(false)
            .wrap(true)
            .selectable(true)
            .xalign(0.0)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        error.add_css_class("error");
        let placeholder = gtk::Label::builder()
            .label("This tab contained a temporary document (for example, a blob or data page). Its content and original address were not saved.\n\nEnter a web address or use Home to continue.")
            .visible(false)
            .wrap(true)
            .selectable(true)
            .hexpand(true)
            .vexpand(true)
            .margin_start(32)
            .margin_end(32)
            .margin_top(32)
            .margin_bottom(32)
            .build();
        let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        page.append(&error);
        page.append(&placeholder);
        page.append(&view);

        let title = gtk::Label::builder()
            .label("New tab")
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .width_chars(16)
            .max_width_chars(24)
            .xalign(0.0)
            .build();
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_tooltip_text(Some("Close tab (Ctrl+W)"));
        let label = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        label.append(&spinner);
        label.append(&title);
        label.append(&close);
        let weak = Rc::downgrade(self);
        close.connect_clicked(move |_| {
            if let Some(browser) = weak.upgrade() {
                browser.close_tab(id);
            }
        });

        self.tabs.borrow_mut().push(Tab {
            id,
            page: page.clone(),
            view: view.clone(),
            title,
            spinner,
            error,
            placeholder,
            generation: Rc::new(Cell::new(0)),
            committed: Rc::new(Cell::new(false)),
        });
        self.connect_view(id, &view);
        let index = self.notebook.append_page(&page, Some(&label));
        self.notebook.set_tab_reorderable(&page, true);
        self.notebook.set_current_page(Some(index));
        if let Some(input) = initial {
            self.navigate(input);
        }
        self.sync_active(true);
        view
    }

    fn close_tab(self: &Rc<Self>, id: u64) {
        let closing_active = self.active_tab().is_some_and(|tab| tab.id == id);
        let removed = {
            let mut tabs = self.tabs.borrow_mut();
            tabs.iter()
                .position(|tab| tab.id == id)
                .map(|index| tabs.remove(index))
        };
        if let Some(tab) = removed {
            self.pending.borrow_mut().remove(&id);
            self.dirty.set(true);
            if let Some(inspector) = tab.view.inspector() {
                inspector.close();
            }
            tab.view.stop_loading();
            if let Some(index) = self.notebook.page_num(&tab.page) {
                self.notebook.remove_page(Some(index));
            }
            if self.notebook.n_pages() == 0 {
                self.add_tab(None, Some("browny:home"));
                self.focus_address();
            } else {
                self.sync_active(closing_active);
            }
        }
    }

    fn sync_active(&self, force_address: bool) {
        if let Some(tab) = self.active_tab() {
            self.sync_controls(&tab, force_address);
        }
    }

    fn sync_controls(&self, tab: &Tab, force_address: bool) {
        // GtkEntry delegates input focus to its internal GtkText child.
        let editing_address = gtk::prelude::RootExt::focus(&self.window)
            .is_some_and(|focus| focus == self.address || focus.is_ancestor(&self.address));
        if force_address || !editing_address {
            let uri = self
                .pending
                .borrow()
                .get(&tab.id)
                .cloned()
                .or_else(|| tab.view.uri().map(|s| s.to_string()))
                .unwrap_or_default();
            self.address.set_text(
                if tab.placeholder.is_visible()
                    || uri == "about:blank"
                    || uri == browny::profile::NON_RESTORABLE_TAB
                {
                    ""
                } else {
                    &uri
                },
            );
        }
        self.reload.set_sensitive(!tab.placeholder.is_visible());
        self.back.set_sensitive(tab.view.can_go_back());
        self.forward.set_sensitive(tab.view.can_go_forward());
        let loading = tab.view.is_loading();
        self.reload.set_icon_name(if loading {
            "process-stop-symbolic"
        } else {
            "view-refresh-symbolic"
        });
        self.reload.set_tooltip_text(Some(if loading {
            "Stop loading (Escape)"
        } else {
            "Reload (Ctrl+R)"
        }));
        self.progress
            .set_fraction(tab.view.estimated_load_progress());
        self.progress.set_visible(loading);
        let title = if tab.placeholder.is_visible() {
            "Not restored".into()
        } else {
            tab.view
                .title()
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "New tab".into())
        };
        let memory = self.memory.borrow();
        let suffix = memory
            .as_ref()
            .filter(|_| self.settings.borrow().show_memory)
            .map(|usage| {
                format!(
                    " · RAM ≈ {} MiB ({}; {} visible processes{})",
                    usage.kib / 1024,
                    usage.metric,
                    usage.processes,
                    if usage.partial { "; partial tree" } else { "" }
                )
            })
            .unwrap_or_default();
        self.window.set_title(Some(&format!(
            "{title} — Browny · {}{}{suffix}",
            self.profile_label,
            if self.profile.private {
                " (private)"
            } else {
                ""
            }
        )));
        let protection = if !self.blocking_enabled(tab.view.uri().as_deref()) {
            "blocking off for this site"
        } else if self.blocker.error.borrow().is_some() {
            "blocking failed — navigation paused"
        } else if !self.blocker.ready.get() {
            "preparing protection"
        } else {
            "starter ad/tracker rules on"
        };
        let download_warning = if self.downloads.borrow().storage_available() {
            ""
        } else {
            " · download records memory-only (see Downloads)"
        };
        let status = format!(
            "Profile: {}{} · {protection}{download_warning}{suffix}",
            self.profile_label,
            if self.profile.private {
                " (private)"
            } else {
                ""
            }
        );
        self.status.set_text(&status);
        self.status.set_tooltip_text(Some("Memory covers only the Linux-visible owned process tree. Shared memory is proportionally counted when PSS is readable; RSS is approximate. Sandbox/reparented processes may be missing. Starter blocking rules are not Brave/uBlock-equivalent protection."));
    }

    fn record_visit(&self, tab: &Tab) {
        if self.closing.get()
            || self.profile.private
            || !self.settings.borrow().remember_history
            || !tab.committed.get()
        {
            return;
        }
        if let Some(uri) = tab.view.uri() {
            let title = tab.view.title().unwrap_or_default();
            if self.library.borrow_mut().visit(&uri, &title, now()) {
                self.dirty.set(true);
            }
        }
    }

    fn update_tab(&self, id: u64) {
        if self.closing.get() {
            return;
        }
        self.dirty.set(true);
        if let Some(tab) = self.tab(id) {
            let title = if tab.placeholder.is_visible() {
                "Not restored".into()
            } else {
                tab.view
                    .title()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| "New tab".into())
            };
            tab.title.set_text(&title);
            let uri = tab.view.uri();
            tab.title.set_tooltip_text(if tab.placeholder.is_visible() {
                None
            } else {
                uri.as_deref()
            });
            let loading = tab.view.is_loading();
            tab.spinner.set_spinning(loading);
            tab.spinner.set_visible(loading);
            if self.active_tab().is_some_and(|active| active.id == id) {
                self.sync_controls(&tab, false);
            }
        }
    }

    fn connect_view(self: &Rc<Self>, id: u64, view: &webkit::WebView) {
        let weak = Rc::downgrade(self);
        view.connect_permission_request(move |_, request| {
            // Most public request types expose no requesting-frame origin.
            // A WebView's top-level URI is not authority for a nested frame.
            // Fail closed rather than silently granting the wrong principal.
            request.deny();
            if let Some(browser) = weak.upgrade()
                && let Some(tab) = browser.tab(id)
            {
                show_error(&tab, &format!(
                    "Permission request blocked ({}): Browny cannot verify the requesting origin for this permission. Per-origin grants are unavailable; no access was allowed.",
                    request.type_().name()
                ));
            }
            true
        });
        for property in ["title", "uri", "is-loading", "estimated-load-progress"] {
            let weak = Rc::downgrade(self);
            view.connect_notify_local(Some(property), move |view, _| {
                if let Some(browser) = weak.upgrade() {
                    if property == "uri"
                        && let Some(tab) = browser.tab(id)
                    {
                        let generation = tab.generation.get();
                        let uri = view.uri();
                        let weak = Rc::downgrade(&browser);
                        // URI notifies also occur for provisional destinations.
                        // Defer until native history has caught up, then require
                        // the current committed item, not just an address change.
                        gtk::glib::idle_add_local_once(move || {
                            if let Some(browser) = weak.upgrade()
                                && let Some(tab) = browser.tab(id)
                                && tab.generation.get() == generation
                                && tab.view.uri() == uri
                                && uri.is_some()
                                && tab
                                    .view
                                    .back_forward_list()
                                    .and_then(|list| list.current_item())
                                    .and_then(|item| item.uri())
                                    == uri
                            {
                                browser.record_visit(&tab);
                            }
                        });
                    }
                    if property == "title"
                        && !browser.profile.private
                        && browser.settings.borrow().remember_history
                        && browser.tab(id).is_some_and(|tab| tab.committed.get())
                        && let Some(uri) = view.uri()
                        && let Some(title) = view.title()
                        && browser.library.borrow_mut().update_title(&uri, &title)
                    {
                        browser.dirty.set(true);
                    }
                    browser.update_tab(id);
                }
            });
        }
        let weak = Rc::downgrade(self);
        view.connect_load_changed(move |_, event| {
            if let Some(browser) = weak.upgrade() {
                if event == webkit::LoadEvent::Started
                    && let Some(tab) = browser.tab(id)
                {
                    tab.error.set_visible(false);
                    tab.generation.set(tab.generation.get().wrapping_add(1));
                    tab.committed.set(false);
                }
                if matches!(
                    event,
                    webkit::LoadEvent::Committed | webkit::LoadEvent::Finished
                ) && let Some(tab) = browser.tab(id)
                {
                    if event == webkit::LoadEvent::Committed {
                        tab.committed.set(true);
                        // A usable document may never finish its subresources.
                        // Record its commit now; later title notifications only
                        // update the existing visit, without reordering history.
                        browser.record_visit(&tab);
                    }
                    // BFCache history restores need not emit a main-resource
                    // response, but do commit the top-level view's final URI.
                    browser.apply_site_blocking(&tab, tab.view.uri().as_deref());
                }
                browser.update_tab(id);
            }
        });
        let weak = Rc::downgrade(self);
        view.connect_load_failed(move |_, _, uri, error| {
            if error.matches(webkit::NetworkError::Cancelled)
                || error.matches(webkit::PolicyError::FrameLoadInterruptedByPolicyChange)
            {
                return false;
            }
            if let Some(browser) = weak.upgrade()
                && let Some(tab) = browser.tab(id)
            {
                tab.committed.set(false);
                show_error(&tab, &format!("Could not load {uri}. {}", error.message()));
                browser.update_tab(id);
            }
            // Mark handled: retain the current page and show our own message.
            // This does not ignore certificate errors or retry insecurely.
            true
        });
        let weak = Rc::downgrade(self);
        view.connect_web_process_terminated(move |_, _| {
            if let Some(browser) = weak.upgrade()
                && let Some(tab) = browser.tab(id)
            {
                show_error(&tab, "The page process stopped. Press Reload to try again.");
                browser.update_tab(id);
            }
        });
        let weak = Rc::downgrade(self);
        view.connect_create(move |source, action| {
            let browser = weak.upgrade()?;
            if !action.is_user_gesture() { return None; }
            if browser.settings.borrow().block_trackers && (!browser.blocker.ready.get() || browser.blocker.filter().is_none()) {
                if let Some(tab) = browser.tab(id) { show_error(&tab, "Protection is not ready. Retry opening the link after it starts, or turn blocking off in Settings."); }
                return None;
            }
            Some(browser.add_tab(Some(source), None).upcast())
        });
        let weak = Rc::downgrade(self);
        view.connect_close(move |_| {
            if let Some(browser) = weak.upgrade() {
                browser.close_tab(id);
            }
        });
        let weak = Rc::downgrade(self);
        view.connect_decide_policy(move |_, decision, kind| {
            let Some(browser) = weak.upgrade() else { return false; };
            let Some(tab) = browser.tab(id) else { decision.ignore(); return true; };
            if kind == webkit::PolicyDecisionType::Response {
                if let Some(response) = decision.downcast_ref::<webkit::ResponsePolicyDecision>()
                    && response.is_main_frame_main_resource() {
                    let uri = response.request().and_then(|request| request.uri());
                    if browser.blocking_enabled(uri.as_deref()) && (!browser.blocker.ready.get() || browser.blocker.filter().is_none()) {
                        decision.ignore();
                        show_error(&tab, "Protection is not ready; this page was stopped before loading subresources.");
                        return true;
                    }
                    // The final main-resource response covers ordinary links,
                    // redirects and history navigations before subresources run.
                    browser.apply_site_blocking(&tab, uri.as_deref());
                    if !response.is_mime_type_supported() { decision.download(); return true; }
                }
            } else if let Some(navigation) = decision.downcast_ref::<webkit::NavigationPolicyDecision>()
                && let Some(action) = navigation.navigation_action()
                && let Some(request) = action.request()
                && let Some(uri) = request.uri()
                && !navigation::is_web_content_uri(&uri)
            {
                decision.ignore();
                let scheme = url::Url::parse(&uri)
                    .map(|url| format!("{}:", url.scheme()))
                    .unwrap_or_else(|_| "unknown".into());
                show_error(&tab, &format!("Unsupported link type: {scheme}. Browny does not open local files or external applications."));
                return true;
            }
            false
        });
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn icon_button(icon: &str, tooltip: &str, action: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.set_action_name(Some(action));
    button
}

fn origin(uri: &str) -> Option<String> {
    let url = url::Url::parse(uri).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.origin().ascii_serialization())
}

fn show_error(tab: &Tab, message: &str) {
    tab.error.set_text(message);
    tab.error.set_visible(true);
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
