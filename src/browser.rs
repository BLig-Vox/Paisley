use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use paisley::navigation;
use webkit::prelude::*;
#[cfg(feature = "cef")]
type View = cef_gtk::WebView;
#[cfg(not(feature = "cef"))]
type View = webkit::WebView;

#[cfg(feature = "cef")]
trait EngineViewBridge {
    fn inspector(&self) -> Option<webkit::WebInspector> {
        None
    }
    fn estimated_load_progress(&self) -> f64;
    fn load_uri(&self, uri: &str);
}
#[cfg(feature = "cef")]
impl EngineViewBridge for View {
    fn estimated_load_progress(&self) -> f64 {
        if self.is_loading() { 0.1 } else { 1.0 }
    }
    fn load_uri(&self, uri: &str) {
        self.load_url(uri);
    }
}

#[derive(Clone)]
struct Tab {
    id: u64,
    #[cfg(feature = "cef")]
    last_used: Rc<Cell<std::time::Instant>>,
    #[cfg(feature = "cef")]
    discarded: Rc<Cell<bool>>,
    page: gtk::Box,
    #[cfg(feature = "cef")]
    split: gtk::Paned,
    view: View,
    title: gtk::Label,
    spinner: gtk::Spinner,
    favicon: gtk::Image,
    error: gtk::Label,
    placeholder: gtk::Label,
    generation: Rc<Cell<u64>>,
    committed: Rc<Cell<bool>>,
    notification_origins: Vec<String>,
    insecure_content: Rc<Cell<bool>>,
}

pub struct Browser {
    window: gtk::ApplicationWindow,
    notebook: gtk::Notebook,
    address: gtk::Entry,
    site_button: gtk::MenuButton,
    site_popover: gtk::Popover,
    site_clear_pending: Cell<bool>,
    back: gtk::Button,
    forward: gtk::Button,
    reload: gtk::Button,
    progress: gtk::ProgressBar,
    #[cfg(not(feature = "cef"))]
    session: webkit::NetworkSession,
    tabs: RefCell<Vec<Tab>>,
    next_id: Cell<u64>,
    memory: RefCell<Option<paisley::memory::Usage>>,
    memory_at: Cell<Option<std::time::Instant>>,
    #[cfg(feature = "cef")]
    residency_epoch: Cell<std::time::Instant>,
    #[cfg(feature = "cef")]
    last_active_tab: Cell<Option<u64>>,
    profile: std::sync::Arc<paisley::profile::Profile>,
    profile_label: String,
    settings: RefCell<paisley::profile::Settings>,
    library: RefCell<paisley::library::Library>,
    pending: RefCell<std::collections::HashMap<u64, String>>,
    dirty: Cell<bool>,
    closing: Cell<bool>,
    writer: RefCell<Option<crate::session_store::SessionWriter>>,
    downloads: RefCell<Rc<crate::downloads::Downloads>>,
    menu: Rc<crate::menu_sheet::MenuSheet>,
    blocker: Rc<crate::blocking::Blocker>,
    favicons: Rc<crate::favicons::Favicons>,
    status: gtk::Label,
    notifications: Rc<crate::notifications::Notifications>,
    notification_context: RefCell<Option<(Vec<String>, webkit::WebContext)>>,
    #[cfg(feature = "cef")]
    native_session: RefCell<Option<Rc<cef_gtk::ProfileSession>>>,
    #[cfg(feature = "cef")]
    native_owner: std::cell::OnceCell<std::rc::Weak<Self>>,
    #[cfg(feature = "cef")]
    native_generation: Cell<u64>,
    #[cfg(feature = "cef")]
    native_operation: Cell<u64>,
    #[cfg(feature = "cef")]
    native_privacy_ready: Cell<Option<bool>>,
    #[cfg(feature = "cef")]
    native_stats: Cell<Option<(u32, u64)>>,
}

impl Browser {
    #[cfg(test)]
    pub fn new(app: &gtk::Application) -> Rc<Self> {
        Self::with_profile(
            app,
            paisley::profile::Profile::open("test-private", true).unwrap(),
        )
        .unwrap()
    }

    pub fn with_profile(
        app: &gtk::Application,
        profile: paisley::profile::Profile,
    ) -> Result<Rc<Self>, String> {
        let restored = if profile.settings.restore_session {
            profile.load_session()?
        } else {
            paisley::profile::Session::default()
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
            .title("Paisley")
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
        let back = icon_button("lucide-arrow-left-symbolic", "Back (Alt+Left)", "win.back");
        let forward = icon_button(
            "lucide-arrow-right-symbolic",
            "Forward (Alt+Right)",
            "win.forward",
        );
        let reload = icon_button("lucide-rotate-cw-symbolic", "Reload (Ctrl+R)", "win.reload");
        let home = icon_button("lucide-house-symbolic", "Home", "win.home");
        let new_tab = icon_button("lucide-plus-symbolic", "New tab (Ctrl+T)", "win.new-tab");
        let header = gtk::HeaderBar::new();
        header.pack_start(&back);
        header.pack_start(&forward);
        header.pack_start(&reload);
        header.pack_start(&home);
        let site_popover = gtk::Popover::builder()
            .autohide(true)
            .has_arrow(true)
            .build();
        let site_button = gtk::MenuButton::builder()
            .icon_name("lucide-shield-symbolic")
            .tooltip_text("Site information and protection")
            .popover(&site_popover)
            .build();
        let address_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        address_bar.add_css_class("linked");
        address_bar.append(&site_button);
        address_bar.append(&address);
        header.set_title_widget(Some(&address_bar));
        header.pack_end(&icon_button(
            "lucide-download-symbolic",
            "Downloads (Ctrl+J)",
            "win.downloads",
        ));
        header.pack_end(&icon_button(
            "lucide-code-xml-symbolic",
            "Web Inspector / Console (F12)",
            "win.inspector",
        ));
        let menu_button = icon_button("lucide-menu-symbolic", "Paisley menu", "win.menu");
        header.pack_end(&menu_button);
        // HeaderBar packs end children inward: add + last, beside the address.
        header.pack_end(&new_tab);
        let bell = icon_button("lucide-bell-symbolic", "Notifications", "win.notifications");
        header.pack_end(&bell);
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
        let overlay = gtk::Overlay::builder()
            .child(&notebook)
            .hexpand(true)
            .vexpand(true)
            .build();
        let notifications = crate::notifications::Notifications::new(
            &overlay,
            &bell,
            &window,
            settings.desktop_notifications && !profile.private,
        );
        root.append(&overlay);
        let status = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .margin_start(8)
            .margin_end(8)
            .build();
        root.append(&status);
        window.set_child(Some(&root));

        #[cfg(not(feature = "cef"))]
        let session = {
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
            if let Some(data) = session.website_data_manager() {
                // WebKit owns icon fetching and the profile/ephemeral cache policy.
                data.set_favicons_enabled(true);
            }
            session.set_itp_enabled(true);
            if settings.strict_privacy
                && let Some(cookies) = session.cookie_manager()
            {
                cookies.set_accept_policy(webkit::CookieAcceptPolicy::NoThirdParty);
            }
            session
        };
        let writer = crate::session_store::SessionWriter::new(std::sync::Arc::clone(&profile));
        let downloads =
            crate::downloads::Downloads::for_profile(&window, download_history, &profile);
        if let Some(error) = download_error {
            downloads.use_memory_records(&error);
        }
        let menu = crate::menu_sheet::MenuSheet::new(&window);
        let blocker = crate::blocking::Blocker::new(&profile)?;
        #[cfg(not(feature = "cef"))]
        let favicon_database = session
            .website_data_manager()
            .and_then(|data| data.favicon_database());
        #[cfg(feature = "cef")]
        let favicon_database = None;
        let favicons = crate::favicons::Favicons::new(
            favicon_database,
            (!profile.private).then(|| profile.cache_dir.join("site-icons-v1")),
            library
                .bookmarks
                .iter()
                .chain(library.history.iter())
                .map(|page| page.uri.clone()),
        );
        let profile_label = match profile.display_name() {
            Ok(name) if name != profile.id => format!("{name} [{}]", profile.id),
            Ok(_) => profile.id.clone(),
            Err(_) => format!("{} (display name unavailable)", profile.id),
        };
        let browser = Rc::new(Self {
            window,
            notebook,
            address,
            site_button,
            site_popover,
            site_clear_pending: Cell::new(false),
            back,
            forward,
            reload,
            progress,
            #[cfg(not(feature = "cef"))]
            session,
            tabs: RefCell::new(Vec::new()),
            next_id: Cell::new(0),
            memory: RefCell::new(None),
            memory_at: Cell::new(None),
            #[cfg(feature = "cef")]
            residency_epoch: Cell::new(std::time::Instant::now()),
            #[cfg(feature = "cef")]
            last_active_tab: Cell::new(None),
            profile,
            profile_label,
            settings: RefCell::new(settings),
            library: RefCell::new(library),
            pending: RefCell::new(std::collections::HashMap::new()),
            dirty: Cell::new(false),
            closing: Cell::new(false),
            writer: RefCell::new(Some(writer)),
            downloads: RefCell::new(downloads),
            menu,
            blocker,
            favicons,
            status,
            notifications,
            notification_context: RefCell::new(None),
            #[cfg(feature = "cef")]
            native_session: RefCell::new(None),
            #[cfg(feature = "cef")]
            native_owner: std::cell::OnceCell::new(),
            #[cfg(feature = "cef")]
            native_generation: Cell::new(0),
            #[cfg(feature = "cef")]
            native_operation: Cell::new(0),
            #[cfg(feature = "cef")]
            native_privacy_ready: Cell::new(None),
            #[cfg(feature = "cef")]
            native_stats: Cell::new(None),
        });
        #[cfg(feature = "cef")]
        {
            browser.native_owner.set(Rc::downgrade(&browser)).unwrap();
            browser.start_native_session();
        }
        let weak = Rc::downgrade(&browser);
        browser.notifications.on_open(move || {
            if let Some(browser) = weak.upgrade() {
                browser.show_notifications();
            }
        });
        browser.install_actions(app);
        browser.track_download_changes();
        let weak = Rc::downgrade(&browser);
        browser.downloads.borrow().on_event(move |title, body| {
            if let Some(browser) = weak.upgrade() {
                let weak = Rc::downgrade(&browser);
                browser.notifications.push(
                    title,
                    body,
                    "Downloads",
                    Some(Rc::new(move || {
                        if let Some(browser) = weak.upgrade() {
                            browser.window.present();
                            browser.menu.dismiss();
                            browser.downloads.borrow().show();
                        }
                    })),
                    None,
                );
            }
        });
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
            browser.add_tab(None, Some("paisley:home"));
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
        browser.sync_active(true);
        browser.start_memory_monitor();
        browser.start_session_writer();
        Ok(browser)
    }

    pub fn present(self: &Rc<Self>) {
        self.window.present();
        self.focus_address();
    }

    pub fn open_external_links(self: &Rc<Self>, urls: &[String]) {
        for uri in urls {
            self.add_tab(None, Some(uri));
        }
        self.window.present();
        if let Some(tab) = self.active_tab() {
            tab.view.grab_focus();
        }
    }

    fn snapshot_session(&self) -> paisley::profile::Session {
        if !self.settings.borrow().restore_session {
            return paisley::profile::Session::default();
        }
        let mut tabs = Vec::new();
        for index in 0..self.notebook.n_pages() {
            if let Some(page) = self.notebook.nth_page(Some(index))
                && let Some(tab) = self.tabs.borrow().iter().find(|tab| tab.page == page)
            {
                if tab.placeholder.is_visible() {
                    tabs.push(paisley::profile::NON_RESTORABLE_TAB.to_owned());
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
                    paisley::profile::session_target(&uri)
                        .unwrap_or("")
                        .to_owned(),
                );
            }
        }
        paisley::profile::Session {
            tabs,
            active: self.notebook.current_page().unwrap_or(0) as usize,
        }
    }

    fn apply_settings(&self) {
        self.notifications
            .set_desktop(self.settings.borrow().desktop_notifications && !self.profile.private);
        #[cfg(feature = "cef")]
        self.send_native_privacy();
        #[cfg(not(feature = "cef"))]
        let strict = self.settings.borrow().strict_privacy;
        #[cfg(not(feature = "cef"))]
        if let Some(cookies) = self.session.cookie_manager() {
            cookies.set_accept_policy(if strict {
                webkit::CookieAcceptPolicy::NoThirdParty
            } else {
                webkit::CookieAcceptPolicy::Always
            });
        }
        for tab in self.tabs.borrow().iter() {
            #[cfg(not(feature = "cef"))]
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

    #[cfg(not(feature = "cef"))]
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

    #[cfg(feature = "cef")]
    fn apply_site_blocking(&self, tab: &Tab, uri: Option<&str>) {
        let settings = self.settings.borrow();
        tab.view.set_blocking(
            settings.block_trackers,
            settings.blocking_exceptions.clone(),
        );
        let _ = uri;
    }

    fn activate_pending(&self, tab: &Tab) {
        #[cfg(feature = "cef")]
        {
            if let Some(previous) = self
                .last_active_tab
                .replace(Some(tab.id))
                .filter(|id| *id != tab.id)
            {
                if let Some(previous) = self.tab(previous) {
                    previous.last_used.set(std::time::Instant::now());
                }
            }
            tab.last_used.set(std::time::Instant::now());
            if tab.discarded.replace(false) {
                if let Some(uri) = tab.view.uri() {
                    self.load_tab(tab, &uri);
                }
            }
        }
        let pending = self.pending.borrow().get(&tab.id).cloned();
        if let Some(uri) = pending {
            self.load_tab(tab, &uri);
        }
    }

    fn load_tab(&self, tab: &Tab, uri: &str) {
        #[cfg(feature = "cef")]
        {
            self.start_native_session();
            tab.discarded.set(false);
            tab.last_used.set(std::time::Instant::now());
        }
        if uri == paisley::profile::NON_RESTORABLE_TAB {
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
            tab.favicon.set_icon_name(Some("lucide-globe-symbolic"));
            tab.favicon.set_visible(true);
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

    #[cfg(feature = "cef")]
    fn reclaim_idle_tabs(&self) {
        let Some(usage) = self.memory.borrow().as_ref().cloned() else {
            return;
        };
        let Some(available) = paisley::memory::available_mib() else {
            return;
        };
        let now = self.residency_epoch.get().elapsed().as_secs();
        let active = self.active_tab().map(|t| t.id);
        let tabs = self.tabs.borrow();
        let candidates: Vec<_> = tabs
            .iter()
            .filter(|t| t.view.has_renderer() && !t.discarded.get())
            .map(|tab| paisley::tab_residency::Tab {
                id: tab.id,
                active: active == Some(tab.id),
                last_used: tab
                    .last_used
                    .get()
                    .checked_duration_since(self.residency_epoch.get())
                    .unwrap_or_default()
                    .as_secs(),
                activity: tab.view.page_activity().map(|(at, protected)| {
                    (
                        at.checked_duration_since(self.residency_epoch.get())
                            .unwrap_or_default()
                            .as_secs(),
                        protected,
                    )
                }),
                reloadable: tab.view.uri().is_some_and(|s| {
                    url::Url::parse(&s).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
                }),
                busy: tab.view.is_loading() || tab.split.end_child().is_some(),
            })
            .collect();
        for id in paisley::tab_residency::victims(&candidates, now, available, usage.kib / 1024) {
            if let Some(tab) = tabs.iter().find(|t| t.id == id) {
                tab.discarded.set(true);
                tab.view.shutdown();
                tab.title
                    .set_tooltip_text(Some("Sleeping tab — reloads when selected"));
            }
        }
    }

    fn start_memory_monitor(self: &Rc<Self>) {
        // One replaceable slot: a busy UI can never build up old readings.
        let latest = std::sync::Arc::new(std::sync::Mutex::new(None));
        let worker = std::sync::Arc::downgrade(&latest);
        std::thread::spawn(move || {
            loop {
                let Some(slot) = worker.upgrade() else {
                    break;
                };
                let started = std::time::Instant::now();
                let usage = paisley::memory::sample(std::process::id());
                if let Ok(mut latest) = slot.lock() {
                    *latest = Some((usage, started));
                }
                drop(slot);
                // /proc work stays off GTK; never overlap sampling passes.
                std::thread::sleep(
                    std::time::Duration::from_secs(1).saturating_sub(started.elapsed()),
                );
            }
        });
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            let Some(browser) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if browser.closing.get() {
                return gtk::glib::ControlFlow::Break;
            }
            if let Ok(mut latest) = latest.try_lock()
                && let Some((usage, sampled_at)) = latest.take()
            {
                // Clear a failed sample instead of presenting an old number as current.
                *browser.memory.borrow_mut() = usage;
                browser.memory_at.set(Some(sampled_at));
                #[cfg(feature = "cef")]
                browser.reclaim_idle_tabs();
            }
            if let Some(tab) = browser.active_tab() {
                browser.sync_status(&tab);
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    fn show_notifications(self: &Rc<Self>) {
        self.downloads.borrow().dismiss();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
        let heading = crate::download_ui::label("Website notifications", "downloads-section");
        content.append(&heading);
        let notice = gtk::Label::builder()
            .label(if self.profile.private {
                "Private window · App alerts are temporary. Website notifications are unavailable in private browsing."
            } else {
                "Alerts arrive while Paisley is running. Recent activity stays in memory."
            })
            .wrap(true)
            .xalign(0.0)
            .build();
        notice.add_css_class("downloads-detail");
        content.append(&notice);
        let current = self
            .active_tab()
            .filter(|tab| !self.profile.private && tab.committed.get())
            .and_then(|tab| tab.view.uri())
            .map(|s| s.to_string());
        if let Some((uri, origin)) =
            current.and_then(|uri| crate::notifications::origin(&uri).map(|origin| (uri, origin)))
        {
            let site = gtk::Label::builder()
                .label(&origin)
                .wrap(true)
                .xalign(0.0)
                .selectable(true)
                .build();
            content.append(&site);
            let allowed = self
                .settings
                .borrow()
                .notification_origins
                .contains(&origin);
            let allow = gtk::Button::with_label(if allowed {
                "Reopen with notifications"
            } else {
                "Allow & open in new tab"
            });
            allow.add_css_class("suggested-action");
            let weak = Rc::downgrade(self);
            allow.connect_clicked(move |_| {
                if let Some(browser) = weak.upgrade() {
                    let mut settings = browser.settings.borrow().clone();
                    if !settings.notification_origins.contains(&origin) {settings.notification_origins.push(origin.clone());}
                    if let Err(error) = browser.profile.save_settings(&settings) {
                        browser.notifications.push("Couldn’t save notification permission", &error, "Paisley", None, None);return;
                    }
                    *browser.settings.borrow_mut()=settings;
                    browser.menu.dismiss();browser.add_tab(None, Some(&uri));
                    browser.notifications.push("Notifications enabled", &format!("Allowed {origin}. The new tab uses this permission; your original tab is unchanged."), "Paisley", None, None);
                }
            });
            content.append(&allow);
        } else {
            let help = gtk::Label::builder()
                .label(if self.profile.private {
                    "Downloads and Paisley alerts still appear here."
                } else {
                    "Visit an HTTPS website to enable its notifications."
                })
                .wrap(true)
                .xalign(0.0)
                .build();
            content.append(&help);
        }
        let origins = self.settings.borrow().notification_origins.clone();
        if !origins.is_empty() {
            content.append(&crate::download_ui::label(
                "Allowed sites",
                "downloads-section",
            ));
        }
        for origin in origins {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let site = crate::download_ui::label(&origin, "downloads-detail");
            site.set_tooltip_text(Some(&origin));
            row.append(&site);
            let revoke = gtk::Button::with_label("Block");
            revoke.add_css_class("flat");
            row.append(&revoke);
            let weak = Rc::downgrade(self);
            revoke.connect_clicked(move |_|{if let Some(browser)=weak.upgrade(){
                let mut settings=browser.settings.borrow().clone();settings.notification_origins.retain(|o|o!=&origin);
                if let Err(error)=browser.profile.save_settings(&settings){browser.notifications.push("Couldn’t save permission",&error,"Paisley",None,None);return;}
                *browser.settings.borrow_mut()=settings;
                browser.notifications.push("Website notifications blocked", "Delivery from older notification contexts has stopped. Reopen tabs to refresh their permission state.", "Paisley", None, None);
                browser.show_notifications();
            }});
            content.append(&row);
        }
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(&crate::download_ui::label(
            "Desktop notifications",
            "downloads-filename",
        ));
        let toggle = gtk::Switch::new();
        toggle.set_valign(gtk::Align::Center);
        toggle.set_active(self.settings.borrow().desktop_notifications && !self.profile.private);
        toggle.set_sensitive(!self.profile.private);
        row.append(&toggle);
        content.append(&row);
        let status = gtk::Label::builder()
            .label(if self.profile.private {
                "Desktop alerts are off in private windows.".into()
            } else {
                self.notifications.desktop_status()
            })
            .wrap(true)
            .xalign(0.0)
            .build();
        status.add_css_class("downloads-detail");
        content.append(&status);
        let weak = Rc::downgrade(self);
        toggle.connect_state_set(move |switch, enabled| {
            if let Some(browser) = weak.upgrade() {
                let mut settings = browser.settings.borrow().clone();
                settings.desktop_notifications = enabled;
                if let Err(error) = browser.profile.save_settings(&settings) {
                    browser.notifications.push(
                        "Couldn’t save notification settings",
                        &error,
                        "Paisley",
                        None,
                        None,
                    );
                    return gtk::glib::Propagation::Stop;
                }
                *browser.settings.borrow_mut() = settings;
                browser
                    .notifications
                    .set_desktop(enabled && !browser.profile.private);
                switch.set_state(enabled);
            }
            gtk::glib::Propagation::Stop
        });
        let system_preview = gtk::Button::with_label("Preview desktop notification");
        system_preview.add_css_class("flat");
        system_preview.set_sensitive(!self.profile.private && toggle.is_active());
        toggle
            .bind_property("active", &system_preview, "sensitive")
            .sync_create()
            .build();
        let weak = Rc::downgrade(self);
        system_preview.connect_clicked(move |_| {
            if let Some(browser) = weak.upgrade() {
                browser.notifications.desktop_preview();
            }
        });
        content.append(&system_preview);
        let preview = gtk::Button::with_label("Preview notification");
        preview.add_css_class("flat");
        let weak = Rc::downgrade(self);
        preview.connect_clicked(move |_|{if let Some(browser)=weak.upgrade(){
            browser.menu.dismiss();browser.notifications.push("You’re all set", "Paisley alerts appear here. Downloads and allowed websites can notify you without interrupting your work.","Paisley",None,None);
        }});
        content.append(&preview);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&self.notifications.panel());
        let scroll = gtk::ScrolledWindow::builder()
            .child(&content)
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        self.menu.show_page("Notifications", &scroll, ());
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
        self.site_popover.connect_show(move |popup| {
            if let Some(browser) = weak.upgrade() {
                popup.set_child(Some(&browser.site_information()));
            }
        });
        let weak = Rc::downgrade(self);
        self.address.connect_activate(move |entry| {
            if let Some(browser) = weak.upgrade() {
                browser.navigate(entry.text().as_str());
            }
        });

        let weak = Rc::downgrade(self);
        self.notebook.connect_switch_page(move |_, page, _| {
            if let Some(browser) = weak.upgrade() {
                browser.site_popover.popdown();
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

        #[cfg(not(feature = "cef"))]
        let weak = Rc::downgrade(self);
        #[cfg(not(feature = "cef"))]
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
                if let Some(writer) = browser.writer.borrow_mut().take() {
                    writer.finish_async(
                        snapshot,
                        browser.library.borrow().clone(),
                        browser.downloads.borrow().storage_snapshot(),
                    );
                }
                let tabs = browser.tabs.replace(Vec::new());
                for tab in tabs {
                    #[cfg(feature = "cef")]
                    Self::close_native_inspector(&tab);
                    if let Some(inspector) = tab.view.inspector() {
                        inspector.close();
                    }
                    tab.view.stop_loading();
                    #[cfg(feature = "cef")]
                    tab.view.shutdown();
                }
                #[cfg(feature = "cef")]
                browser.native_session.borrow_mut().take();
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
                crate::library_ui::Change::SaveBookmark { uri, title } => {
                    browser.library.borrow_mut().bookmark(&uri, &title, now())?;
                }
                crate::library_ui::Change::ClearHistory => {
                    browser.library.borrow_mut().history.clear()
                }
                crate::library_ui::Change::RemoveBookmark(uri) => {
                    browser.library.borrow_mut().remove_bookmark(&uri)
                }
                crate::library_ui::Change::ImportBookmarks(import) => {
                    let next = paisley::bookmark_transfer::merge_import(
                        &browser.library.borrow(),
                        &import,
                    )?;
                    *browser.library.borrow_mut() = next;
                }
            }
            browser.dirty.set(true);
            Ok(())
        });
        self.downloads.borrow().dismiss();
        crate::library_ui::show_in_sheet(
            &self.window,
            bookmarks,
            self.profile.private,
            read,
            open,
            update,
            &self.menu,
            Some(Rc::clone(&self.favicons)),
        )
    }

    fn show_bookmark(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let uri = tab.view.uri().unwrap_or_default().to_string();
        let normalized = url::Url::parse(&uri)
            .ok()
            .map(|url| url.to_string())
            .unwrap_or(uri);
        let existing = self
            .library
            .borrow()
            .bookmarks
            .iter()
            .find(|page| page.uri == normalized)
            .cloned();
        let title = existing
            .as_ref()
            .map(|page| page.title.clone())
            .unwrap_or_else(|| tab.view.title().unwrap_or_default().to_string());
        let weak = Rc::downgrade(self);
        let uri = normalized.clone();
        let save = Rc::new(move |title: &str| {
            let browser = weak.upgrade().ok_or("Browser has closed")?;
            browser.library.borrow_mut().bookmark(&uri, title, now())?;
            browser.dirty.set(true);
            Ok(())
        });
        let weak = Rc::downgrade(self);
        let uri = normalized.clone();
        let remove = Rc::new(move || {
            let browser = weak.upgrade().ok_or("Browser has closed")?;
            browser.library.borrow_mut().remove_bookmark(&uri);
            browser.dirty.set(true);
            Ok(())
        });
        self.downloads.borrow().dismiss();
        crate::browsing_ui::bookmark(
            &self.menu,
            &normalized,
            &title,
            existing.is_some(),
            self.profile.private,
            save,
            remove,
        );
    }

    fn set_site_protection(&self, site: &str, enabled: bool) -> Result<(), String> {
        let mut settings = self.settings.borrow().clone();
        if !settings.block_trackers {
            return Err("Turn on blocking in Settings first".into());
        }
        settings.blocking_exceptions.retain(|item| item != site);
        if !enabled {
            settings.blocking_exceptions.push(site.to_owned());
        }
        self.profile.save_settings(&settings)?;
        *self.settings.borrow_mut() = settings;
        self.apply_settings();
        Ok(())
    }
    fn site_information(self: &Rc<Self>) -> gtk::Box {
        let tab = self.active_tab();
        let uri = tab
            .as_ref()
            .and_then(|t| t.view.uri())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let parsed = url::Url::parse(&uri).ok();
        let supported = tab.as_ref().is_some_and(|t| t.committed.get())
            && parsed
                .as_ref()
                .is_some_and(|u| matches!(u.scheme(), "http" | "https"));
        let site = origin(&uri);
        let origin_label = if supported {
            site.clone().unwrap_or_else(|| "This page".into())
        } else {
            "No committed website".into()
        };
        let (connection, certificate) = if !supported {
            (
                "Connection details unavailable".into(),
                "Open a website to inspect its connection.".into(),
            )
        } else if parsed.as_ref().is_some_and(|u| u.scheme() == "https") {
            match tab.as_ref().and_then(|t| t.view.tls_info()) {
                Some((cert, errors)) if errors.is_empty() => {
                    let subject = cert
                        .subject_name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "Unavailable".into());
                    let issuer = cert
                        .issuer_name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "Unavailable".into());
                    (
                        if tab.as_ref().is_some_and(|t| {
                            #[cfg(feature = "cef")]
                            {
                                t.view.has_insecure_content()
                            }
                            #[cfg(not(feature = "cef"))]
                            {
                                t.insecure_content.get()
                            }
                        }) {
                            "HTTPS with insecure content".into()
                        } else {
                            "Connection secure".into()
                        },
                        format!(
                            "The browser engine validated this HTTPS connection.
Subject: {}
Issuer: {}
This describes transport security, not the trustworthiness of the website.",
                            subject.chars().take(500).collect::<String>(),
                            issuer.chars().take(500).collect::<String>()
                        ),
                    )
                }
                Some((_, errors)) => (
                    "Connection has certificate errors".into(),
                    format!(
                        "The browser engine reported certificate errors: {errors:?}. Paisley does not bypass these errors."
                    ),
                ),
                None => (
                    "Connection not yet verified".into(),
                    "The browser engine has not supplied validated TLS details for this page."
                        .into(),
                ),
            }
        } else {
            (
                "Connection not encrypted".into(),
                "This page uses HTTP. Data exchanged with the site is not protected by TLS.".into(),
            )
        };
        let weak = Rc::downgrade(self);
        let toggle = Rc::new(move |enabled| {
            let browser = weak.upgrade().ok_or("Browser closed")?;
            browser.set_site_protection(site.as_deref().ok_or("Open a website first")?, enabled)
        });
        let weak = Rc::downgrade(self);
        let clear = Rc::new(move || {
            let Some(browser) = weak.upgrade() else {
                return;
            };
            #[cfg(feature = "cef")]
            {
                if let Some(origin) = parsed
                    .as_ref()
                    .filter(|u| matches!(u.scheme(), "http" | "https"))
                    .map(|u| u.origin().ascii_serialization())
                {
                    browser.confirm_native_clear(origin);
                }
            }
            #[cfg(not(feature = "cef"))]
            {
                let Some(host) = parsed
                    .as_ref()
                    .and_then(|u| u.host_str())
                    .map(str::to_ascii_lowercase)
                else {
                    return;
                };
                let Some(manager) = browser.session.website_data_manager() else {
                    return;
                };
                if browser.site_clear_pending.replace(true) {
                    return;
                }
                let window = browser.window.downgrade();
                let weak = Rc::downgrade(&browser);
                gtk::glib::MainContext::default().spawn_local(async move {
                    let Some(window) = window.upgrade() else {
                        return;
                    };
                    let result = crate::site_info::clear_site_data(&window, &manager, &host).await;
                    if let Some(browser) = weak.upgrade() {
                        browser.site_clear_pending.set(false);
                        match result {
                            Ok(message) => browser.notifications.push(
                                "Site data",
                                &message,
                                "Paisley",
                                None,
                                None,
                            ),
                            Err(error) => browser.notifications.push(
                                "Couldn’t clear site data",
                                &error,
                                "Paisley",
                                None,
                                None,
                            ),
                        }
                    }
                });
            }
        });
        crate::site_info::content(
            crate::site_info::SiteInfo {
                origin: origin_label,
                connection,
                certificate,
                supported,
                enabled: self.blocking_enabled(Some(&uri)),
                available: self.blocker.ready.get() && self.blocker.filter().is_some(),
                global: self.settings.borrow().block_trackers,
            },
            toggle,
            clear,
            &self.site_popover,
        )
    }

    fn show_protection(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let uri = tab.view.uri().unwrap_or_default().to_string();
        let site = origin(&uri);
        let weak = Rc::downgrade(self);
        let update = Rc::new(move |enabled: bool| {
            let browser = weak.upgrade().ok_or("Browser has closed")?;
            let site = site.as_ref().ok_or("Open a website to change protection")?;
            browser.set_site_protection(site, enabled)
        });
        self.downloads.borrow().dismiss();
        crate::browsing_ui::protection(
            &self.menu,
            &uri,
            self.blocking_enabled(Some(&uri)),
            self.settings.borrow().block_trackers,
            self.blocker.ready.get() && self.blocker.filter().is_some(),
            update,
        );
    }

    fn install_actions(self: &Rc<Self>, app: &gtk::Application) {
        self.action("bookmark-current", |browser| browser.show_bookmark());
        self.action("bookmarks", |browser| {
            browser.show_library(true);
        });
        self.action("history", |browser| {
            browser.show_library(false);
        });
        self.action("settings", |browser| {
            let weak = Rc::downgrade(browser);
            browser.downloads.borrow().dismiss();
            crate::settings_ui::show_in_sheet(
                &browser.window,
                browser.settings.borrow().clone(),
                move |settings| {
                    let browser = weak.upgrade().ok_or("Browser has closed")?;
                    browser.profile.save_settings(&settings)?;
                    *browser.settings.borrow_mut() = settings;
                    browser.apply_settings();
                    Ok(())
                },
                &browser.menu,
            );
        });
        self.action("profiles", |browser| {
            browser.downloads.borrow().dismiss();
            crate::settings_ui::profiles_in_sheet(
                &browser.window,
                &browser.profile.id,
                browser.profile.private,
                &browser.menu,
            )
        });
        self.action("site-blocking", |browser| browser.show_protection());
        self.action("notifications", |browser| browser.show_notifications());
        self.action("new-tab", |browser| {
            browser.add_tab(None, Some("paisley:home"));
            browser.focus_address();
        });
        self.action("close-tab", |browser| {
            if let Some(tab) = browser.active_tab() {
                browser.close_tab(tab.id);
            }
        });
        self.action("menu", |browser| {
            browser.downloads.borrow().dismiss();
            browser.menu.toggle();
        });
        self.action("downloads", |browser| {
            browser.menu.back();
            browser.downloads.borrow().toggle();
            if browser.downloads.borrow().is_open() {
                browser.menu.select("Downloads");
            }
        });
        self.action("inspector", |browser| {
            #[cfg(feature = "cef")]
            {
                browser.toggle_native_inspector();
                return;
            }
            #[cfg(not(feature = "cef"))]
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
        self.action("home", |browser| browser.navigate("paisley:home"));
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
                #[cfg(feature = "cef")]
                browser.start_native_session();
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
                    #[cfg(feature = "cef")]
                    browser.start_native_session();
                    tab.view.reload();
                }
            }
        });
        self.reload.set_action_name(Some("win.reload-or-stop"));
        self.action("stop", |browser| {
            if browser.downloads.borrow().dismiss() || browser.menu.escape() {
                return;
            }
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
        let dismiss_menu = !matches!(
            name,
            "menu" | "stop" | "settings" | "profiles" | "bookmarks" | "history" | "downloads"
        );
        action.connect_activate(move |_, _| {
            if let Some(browser) = weak.upgrade() {
                if dismiss_menu {
                    browser.menu.dismiss();
                }
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
        self.site_popover.popdown();
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

    #[cfg(not(feature = "cef"))]
    fn create_view(&self, related: Option<&View>) -> (View, Vec<String>) {
        let manager = crate::site_styles::content_manager();
        // Page-created related views can navigate before load_tab is called.
        // Install protection on their content manager before returning the view.
        if self.settings.borrow().block_trackers
            && let Some(filter) = self.blocker.filter()
        {
            manager.add_filter(&filter);
        }
        let builder = webkit::WebView::builder().user_content_manager(&manager);
        let notification_origins = related
            .and_then(|source| {
                self.tabs
                    .borrow()
                    .iter()
                    .find(|tab| tab.view == *source)
                    .map(|tab| tab.notification_origins.clone())
            })
            .unwrap_or_else(|| self.settings.borrow().notification_origins.clone());
        let view = match related {
            Some(source) => builder.related_view(source).build(),
            None => {
                let mut cached = self.notification_context.borrow_mut();
                if cached
                    .as_ref()
                    .is_none_or(|(grants, _)| *grants != notification_origins)
                {
                    let context = crate::engine_memory::context();
                    let origins: Vec<_> = notification_origins
                        .iter()
                        .map(|uri| webkit::SecurityOrigin::for_uri(uri))
                        .collect();
                    context.initialize_notification_permissions(
                        &origins.iter().collect::<Vec<_>>(),
                        &[],
                    );
                    context.connect_initialize_notification_permissions(move |context| {
                        context.initialize_notification_permissions(
                            &origins.iter().collect::<Vec<_>>(),
                            &[],
                        );
                    });
                    *cached = Some((notification_origins.clone(), context));
                }
                builder
                    .network_session(&self.session)
                    .web_context(&cached.as_ref().unwrap().1)
                    .build()
            }
        };
        if let Some(settings) = webkit::prelude::WebViewExt::settings(&view) {
            settings.set_enable_developer_extras(true);
            // Back/Forward keeps URL history, but not entire prior DOM/JS heaps.
            // Live tabs remain resident; this does not discard open page state.
            settings.set_enable_page_cache(false);
            settings.set_media_playback_requires_user_gesture(true);
            settings.set_javascript_can_open_windows_automatically(false);
        }
        if self.settings.borrow().strict_privacy
            && let Some(settings) = webkit::prelude::WebViewExt::settings(&view)
        {
            settings.set_enable_webrtc(false);
        }
        (view, notification_origins)
    }

    #[cfg(feature = "cef")]
    fn send_native_privacy(&self) {
        self.native_privacy_ready.set(None);
        if let Some(session) = self.native_session.borrow().as_ref() {
            session.send(cef_gtk::proto_ipc::servo_action::Action::StrictPrivacy(
                self.settings.borrow().strict_privacy,
            ));
        }
    }
    #[cfg(feature = "cef")]
    fn request_native_clear(&self, page_id: u64, origin: String) {
        self.start_native_session();
        let id = self.native_operation.get().wrapping_add(1);
        self.native_operation.set(id);
        self.site_clear_pending.set(true);
        if let Some(session) = self.native_session.borrow().as_ref() {
            session.send(cef_gtk::proto_ipc::servo_action::Action::ClearSiteData(
                cef_gtk::proto_ipc::ClearSiteData {
                    request_id: id,
                    page_id,
                    origin,
                },
            ));
        }
    }
    #[cfg(feature = "cef")]
    fn confirm_native_clear(self: &Rc<Self>, origin: String) {
        if self.site_clear_pending.replace(true) {
            return;
        }
        let Some(tab) = self.active_tab() else {
            self.site_clear_pending.set(false);
            return;
        };
        let Some(page_id) = tab.view.page_id() else {
            self.site_clear_pending.set(false);
            return;
        };
        let parent = self.window.downgrade();
        let weak = Rc::downgrade(self);
        gtk::glib::spawn_future_local(async move {
            let Some(parent) = parent.upgrade() else {
                return;
            };
            let dialog=gtk::AlertDialog::builder().message(format!("Clear data for {origin}?"))
                .detail("Cookies, site storage, caches and service workers for this origin will be removed in this profile. This may sign you out. Open tabs may recreate data.")
                .buttons(["Cancel","Clear site data"]).cancel_button(0).default_button(0).modal(true).build();
            let accepted = dialog.choose_future(Some(&parent)).await.ok() == Some(1);
            if let Some(browser) = weak.upgrade() {
                if accepted && !browser.closing.get() {
                    browser.request_native_clear(page_id, origin);
                } else {
                    browser.site_clear_pending.set(false);
                }
            }
        });
    }

    #[cfg(feature = "cef")]
    fn engine_storage(&self) -> std::path::PathBuf {
        if self.profile.private {
            std::path::PathBuf::from(format!("private-window-{:p}", self))
        } else {
            self.profile.data_dir.join("cef")
        }
    }
    #[cfg(feature = "cef")]
    fn start_native_session(&self) {
        if self.closing.get()
            || self
                .native_session
                .borrow()
                .as_ref()
                .is_some_and(|session| !session.is_closed())
        {
            return;
        }
        let weak = self
            .native_owner
            .get()
            .expect("browser owner initialized")
            .clone();
        let storage = self.engine_storage();
        let generation = self.native_generation.get().wrapping_add(1);
        self.native_generation.set(generation);
        let session = Rc::new(cef_gtk::ProfileSession::new(
            storage.to_str().expect("UTF-8 profile path"),
            self.profile.private,
            move |event| {
                if let Some(browser) = weak.upgrade() {
                    if browser.closing.get() || browser.native_generation.get() != generation {
                        return;
                    }
                    match event.event {
                        Some(cef_gtk::proto_ipc::servo_event::Event::EngineStats(stats)) => browser
                            .native_stats
                            .set(Some((stats.pages, stats.closed_pages))),
                        Some(cef_gtk::proto_ipc::servo_event::Event::OperationResult(result)) => {
                            if result.request_id == u64::MAX {
                                browser.native_privacy_ready.set(Some(result.success));
                                if !result.success {
                                    browser.notifications.push(
                                        "Privacy settings",
                                        &result.message,
                                        "Paisley",
                                        None,
                                        None,
                                    );
                                }
                                return;
                            }
                            if result.request_id == browser.native_operation.get() {
                                browser.site_clear_pending.set(false);
                                browser.notifications.push(
                                    if result.success {
                                        "Site data"
                                    } else {
                                        "Couldn’t clear site data"
                                    },
                                    &result.message,
                                    "Paisley",
                                    None,
                                    None,
                                );
                            }
                        }
                        Some(event) => browser.downloads.borrow().native_event(event),
                        None => {
                            browser.downloads.borrow().native_disconnected();
                            if browser.site_clear_pending.replace(false) {
                                browser.notifications.push(
                                    "Couldn’t clear site data",
                                    "The browser engine stopped. Reload the page and try again.",
                                    "Paisley",
                                    None,
                                    None,
                                );
                            }
                        }
                    }
                }
            },
        ));
        self.downloads.borrow().bind_native(session.clone());
        self.native_session.replace(Some(session));
        self.send_native_privacy();
    }

    #[cfg(feature = "cef")]
    fn create_view(&self, _related: Option<&View>) -> (View, Vec<String>) {
        let storage = self.engine_storage();
        let view = View::for_profile(
            storage.to_str().expect("profile path is UTF-8"),
            self.profile.private,
        );
        (view, self.settings.borrow().notification_origins.clone())
    }

    fn add_tab(self: &Rc<Self>, related: Option<&View>, initial: Option<&str>) -> View {
        let (view, origins) = self.create_view(related);
        self.add_view_tab(view, origins, initial)
    }

    fn add_view_tab(
        self: &Rc<Self>,
        view: View,
        notification_origins: Vec<String>,
        initial: Option<&str>,
    ) -> View {
        #[cfg(feature = "cef")]
        self.start_native_session();
        let id = self.next_id.get();
        self.next_id.set(id + 1);
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
        let split = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .vexpand(true)
            .start_child(&view)
            .resize_start_child(true)
            .resize_end_child(true)
            .shrink_start_child(true)
            .shrink_end_child(true)
            .build();
        #[cfg(not(feature = "cef"))]
        crate::inspector_dock::install(&view, &split);
        page.append(&split);

        let title = gtk::Label::builder()
            .label("New tab")
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .width_chars(16)
            .max_width_chars(24)
            .xalign(0.0)
            .build();
        let favicon = gtk::Image::from_icon_name("lucide-globe-symbolic");
        favicon.set_pixel_size(16);
        favicon.set_size_request(16, 16);
        let spinner = gtk::Spinner::new();
        spinner.set_size_request(16, 16);
        spinner.set_visible(false);
        let close = gtk::Button::from_icon_name("lucide-x-symbolic");
        close.add_css_class("flat");
        close.set_tooltip_text(Some("Close tab (Ctrl+W)"));
        let label = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        label.append(&favicon);
        label.append(&spinner);
        label.append(&title);
        label.append(&close);
        let weak = Rc::downgrade(self);
        close.connect_clicked(move |_| {
            if let Some(browser) = weak.upgrade() {
                browser.close_tab(id);
            }
        });

        let middle = gtk::GestureClick::new();
        middle.set_button(2);
        middle.set_propagation_phase(gtk::PropagationPhase::Capture);
        middle.connect_pressed(|gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        let weak = Rc::downgrade(self);
        middle.connect_released(move |_, _, _, _| {
            if let Some(browser) = weak.upgrade() {
                browser.close_tab(id);
            }
        });
        label.add_controller(middle);

        self.tabs.borrow_mut().push(Tab {
            id,
            #[cfg(feature = "cef")]
            last_used: Rc::new(Cell::new(std::time::Instant::now())),
            #[cfg(feature = "cef")]
            discarded: Rc::new(Cell::new(false)),
            page: page.clone(),
            #[cfg(feature = "cef")]
            split: split.clone(),
            view: view.clone(),
            title,
            spinner,
            favicon,
            error,
            placeholder,
            generation: Rc::new(Cell::new(0)),
            committed: Rc::new(Cell::new(false)),
            notification_origins,
            insecure_content: Rc::new(Cell::new(false)),
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

    #[cfg(feature = "cef")]
    fn close_native_inspector(tab: &Tab) {
        if let Some(frontend) = tab
            .split
            .end_child()
            .and_then(|w| w.downcast::<View>().ok())
        {
            frontend.shutdown();
        }
        tab.split.set_end_child(gtk::Widget::NONE);
    }
    #[cfg(feature = "cef")]
    fn toggle_native_inspector(&self) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        if tab.split.end_child().is_some() {
            Self::close_native_inspector(&tab);
            return;
        }
        let Some(frontend) = View::inspector_for(&tab.view) else {
            return;
        };
        frontend.set_hexpand(true);
        frontend.set_vexpand(true);
        let weak = tab.split.downgrade();
        frontend.connect_renderer_terminated(move |_| {
            if let Some(split) = weak.upgrade() {
                split.set_end_child(gtk::Widget::NONE);
            }
        });
        tab.split.set_end_child(Some(&frontend));
        let width = tab.split.width().max(1);
        let tools = (width * 2 / 5).clamp(320, 600).min(width / 2);
        tab.split.set_position(width - tools);
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
            #[cfg(feature = "cef")]
            Self::close_native_inspector(&tab);
            #[cfg(feature = "cef")]
            tab.view.shutdown();
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
                self.add_tab(None, Some("paisley:home"));
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
                    || uri == paisley::profile::NON_RESTORABLE_TAB
                {
                    ""
                } else {
                    &uri
                },
            );
        }
        let uri = tab.view.uri().unwrap_or_default();
        let secure = !tab.insecure_content.get()
            && tab.committed.get()
            && uri.starts_with("https:")
            && tab
                .view
                .tls_info()
                .is_some_and(|(_, errors)| errors.is_empty());
        self.site_button.set_icon_name(if secure {
            "lucide-shield-check-symbolic"
        } else {
            "lucide-shield-symbolic"
        });
        self.reload.set_sensitive(!tab.placeholder.is_visible());
        self.back.set_sensitive(tab.view.can_go_back());
        self.forward.set_sensitive(tab.view.can_go_forward());
        let loading = tab.view.is_loading();
        self.reload.set_icon_name(if loading {
            "lucide-square-symbolic"
        } else {
            "lucide-rotate-cw-symbolic"
        });
        self.reload.set_tooltip_text(Some(if loading {
            "Stop loading (Escape)"
        } else {
            "Reload (Ctrl+R)"
        }));
        self.progress
            .set_fraction(tab.view.estimated_load_progress());
        self.progress.set_visible(loading);
        self.sync_status(tab);
    }

    fn sync_status(&self, tab: &Tab) {
        let title = if tab.placeholder.is_visible() {
            "Not restored".into()
        } else {
            tab.view
                .title()
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "New tab".into())
        };
        let memory = self.memory.borrow();
        let age = self.memory_at.get().map(|at| at.elapsed());
        let stale = age.is_some_and(|age| age > std::time::Duration::from_secs(3));
        let suffix = if !self.settings.borrow().show_memory {
            String::new()
        } else if let Some(usage) = memory.as_ref() {
            format!(
                " · Paisley total ≈ {:.1} MiB ({}; {} processes{}{})",
                usage.kib as f64 / 1024.0,
                usage.metric,
                usage.processes,
                if usage.partial { "; partial tree" } else { "" },
                if stale { "; stale" } else { "" }
            )
        } else if age.is_some() {
            " · Paisley memory unavailable".into()
        } else {
            " · Measuring Paisley memory…".into()
        };
        let window_title = format!(
            "{title} — Paisley · {}{}{suffix}",
            self.profile_label,
            if self.profile.private {
                " (private)"
            } else {
                ""
            }
        );
        if self.window.title().as_deref() != Some(window_title.as_str()) {
            self.window.set_title(Some(&window_title));
        }
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
        if self.status.text().as_str() != status {
            self.status.set_text(&status);
        }
        let freshness = age
            .map(|age| format!("Last sample started {:.1} seconds ago.", age.as_secs_f64()))
            .unwrap_or_else(|| "Waiting for the first sample.".into());
        self.status.set_tooltip_text(Some(&format!("Paisley memory is sampled about once per second on a background thread. {freshness} Readings older than three seconds are marked stale; unavailable readings are cleared. PSS proportionally counts shared pages; an unreadable process uses RSS instead, which can overcount shared memory. Sandboxed or reparented processes may be missing. This estimates Paisley's Linux-visible process tree, not total system RAM. Starter blocking rules are not Brave/uBlock-equivalent protection.")));
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
            if tab.committed.get()
                && !tab.placeholder.is_visible()
                && let Some(texture) = tab.view.favicon()
            {
                tab.favicon.set_paintable(Some(&texture));
            } else {
                tab.favicon.set_icon_name(Some("lucide-globe-symbolic"));
            }
            tab.favicon.set_visible(!loading);
            if self.active_tab().is_some_and(|active| active.id == id) {
                self.sync_controls(&tab, false);
            }
        }
    }

    #[cfg(feature = "cef")]
    fn connect_view(self: &Rc<Self>, id: u64, view: &View) {
        let weak = Rc::downgrade(self);
        view.connect_page_error(move |_, message| {
            if let Some(browser) = weak.upgrade() {
                if let Some(tab) = browser.tab(id) {
                    tab.committed.set(false);
                    show_error(&tab, message);
                    browser.update_tab(id);
                }
            }
        });
        for property in [
            "title",
            "uri",
            "is-loading",
            "can-go-back",
            "can-go-forward",
            "favicon",
        ] {
            let weak = Rc::downgrade(self);
            view.connect_notify_local(Some(property), move |view, _| {
                if let Some(browser) = weak.upgrade() {
                    if property == "favicon" {
                        if let (Some(uri), Some(texture)) = (view.uri(), view.favicon()) {
                            browser.favicons.capture(&uri, texture);
                        }
                    }
                    browser.update_tab(id);
                }
            });
        }
        let weak = Rc::downgrade(self);
        view.connect_load_changed(move |_, event| {
            if let Some(browser) = weak.upgrade()
                && let Some(tab) = browser.tab(id)
            {
                if matches!(event, cef_gtk::LoadEvent::Finished)
                    && !browser.profile.private && !tab.error.is_visible()
                    && let Some(uri) = tab.view.uri()
                {
                    for script in paisley::native_extensions::scripts(&browser.profile.data_dir, &uri) {
                        tab.view.evaluate_javascript(&script);
                    }
                }
                match event {
                    cef_gtk::LoadEvent::Started => {
                        tab.committed.set(false);
                        tab.error.set_visible(false);
                        tab.generation.set(tab.generation.get().wrapping_add(1));
                    }
                    cef_gtk::LoadEvent::Committed | cef_gtk::LoadEvent::Finished => {
                        if !tab.error.is_visible() {
                            tab.committed.set(true);
                            browser.record_visit(&tab);
                        }
                    }
                }
                browser.update_tab(id);
            }
        });
        let weak = Rc::downgrade(self);
        view.connect_create_web_view(move |opener, popup_id| {
            if let Some(browser) = weak.upgrade() {
                let (popup, origins) = browser.create_view(None);
                if popup.attach_popup(opener, popup_id) {
                    browser.add_view_tab(popup, origins, None);
                }
            }
        });
        let weak = Rc::downgrade(self);
        view.connect_popup_closed(move || {
            if let Some(browser) = weak.upgrade() {
                browser.close_tab(id);
            }
        });
        let weak = Rc::downgrade(self);
        view.connect_renderer_terminated(move |_| {
            if let Some(browser) = weak.upgrade()
                && let Some(tab) = browser.tab(id)
            {
                show_error(&tab, "The page process stopped. Reload to reopen it.");
                browser.update_tab(id);
            }
        });
    }

    #[cfg(not(feature = "cef"))]
    fn connect_view(self: &Rc<Self>, id: u64, view: &webkit::WebView) {
        let weak = Rc::downgrade(self);
        view.connect_permission_request(move |_, request| {
            // Generic requests cannot establish a requesting-frame origin.
            // Website grants come only from explicit native origin selection
            // and WebKit context initialization, never a top-frame inference.
            request.deny();
            if let Some(browser) = weak.upgrade() {
                if request.is::<webkit::NotificationPermissionRequest>() {
                    browser.notifications.push("Website notifications are off", "Open Notifications to explicitly allow a site, then reopen it in a fresh tab.", "Paisley", None, None);
                } else if let Some(tab) = browser.tab(id) {
                    show_error(&tab, &format!("Permission request blocked ({}): Paisley cannot verify the requesting origin for this permission. No access was allowed.", request.type_().name()));
                }
            }
            true
        });
        let weak = Rc::downgrade(self);
        view.connect_show_notification(move |_, notification| {
            let Some(browser) = weak.upgrade() else {
                notification.close();
                return true;
            };
            let Some(tab) = browser.tab(id) else {
                notification.close();
                return true;
            };
            // Notification objects expose no authenticated sending origin.
            // After revocation, reject older contexts with a broader grant set.
            let permitted = !browser.closing.get()
                && !tab.notification_origins.is_empty()
                && tab.notification_origins.iter().all(|origin| {
                    browser
                        .settings
                        .borrow()
                        .notification_origins
                        .contains(origin)
                });
            if !permitted {
                notification.close();
                return true;
            }
            let clicked = notification.clone();
            let weak = Rc::downgrade(&browser);
            browser.notifications.push(
                notification
                    .title()
                    .as_deref()
                    .unwrap_or("Website notification"),
                notification.body().as_deref().unwrap_or(""),
                "Website notification",
                Some(Rc::new(move || {
                    if let Some(browser) = weak.upgrade()
                        && let Some(tab) = browser.tab(id)
                    {
                        browser.window.present();
                        browser.menu.dismiss();
                        if let Some(index) = browser.notebook.page_num(&tab.page) {
                            browser.notebook.set_current_page(Some(index));
                        }
                        tab.view.grab_focus();
                        clicked.clicked();
                    }
                })),
                Some(notification.clone()),
            );
            true
        });
        let weak = Rc::downgrade(self);
        view.connect_insecure_content_detected(move |_, _| {
            if let Some(browser) = weak.upgrade()
                && let Some(tab) = browser.tab(id)
            {
                tab.insecure_content.set(true);
                browser.site_popover.popdown();
                if browser.active_tab().is_some_and(|t| t.id == id) {
                    browser.sync_controls(&tab, false);
                }
            }
        });
        for property in [
            "title",
            "uri",
            "is-loading",
            "estimated-load-progress",
            "favicon",
        ] {
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
                    browser.site_popover.popdown();
                    tab.error.set_visible(false);
                    tab.generation.set(tab.generation.get().wrapping_add(1));
                    tab.committed.set(false);
                    tab.insecure_content.set(false);
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
                    if event == webkit::LoadEvent::Finished
                        && tab.committed.get()
                        && let Some(uri) = tab.view.uri()
                    {
                        browser.favicons.revisit(&uri);
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
                browser.notifications.push(
                    "This tab stopped responding",
                    "Reload the page to try again. Other tabs are still available.",
                    "Paisley",
                    None,
                    None,
                );
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
                show_error(&tab, &format!("Unsupported link type: {scheme}. Paisley does not open local files or external applications."));
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

#[cfg(all(test, not(feature = "cef")))]
#[path = "browser_tests.rs"]
mod tests;

#[cfg(all(test, feature = "cef-test-support"))]
#[path = "cef_browser_tests.rs"]
pub(crate) mod engine_tests;
