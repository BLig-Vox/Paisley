//! Bounded, window-local event history. No event contents are persisted.
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};

struct Event {
    id: u64,
    unread: bool,
    title: String,
    body: String,
    source: String,
    time: String,
    action: Option<Rc<dyn Fn()>>,
    web: Option<webkit::Notification>,
}

pub struct Notifications {
    events: RefCell<VecDeque<Event>>,
    next: Cell<u64>,
    badge: gtk::Label,
    banner: gtk::Revealer,
    banner_title: gtk::Label,
    banner_body: gtk::Label,
    timeout: RefCell<Option<gtk::glib::SourceId>>,
    list: RefCell<Option<gtk::glib::WeakRef<gtk::Box>>>,
    last_web: Cell<Option<Instant>>,
    parent: gtk::glib::WeakRef<gtk::ApplicationWindow>,
    desktop: RefCell<Option<Rc<crate::desktop_notifications::DesktopNotifications>>>,
    open: RefCell<Option<Rc<dyn Fn()>>>,
    last_notice: RefCell<Option<(String, String, Instant)>>,
}

pub fn origin(uri: &str) -> Option<String> {
    let url = url::Url::parse(uri).ok()?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url.origin().ascii_serialization())
}
fn text(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max)
        .collect()
}
impl Notifications {
    pub fn new(
        overlay: &gtk::Overlay,
        button: &gtk::Button,
        parent: &gtk::ApplicationWindow,
        desktop_enabled: bool,
    ) -> Rc<Self> {
        let badge = gtk::Label::new(None);
        badge.add_css_class("notification-badge");
        badge.set_visible(false);
        let child = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let icon = gtk::Image::from_icon_name("lucide-bell-symbolic");
        icon.set_pixel_size(18);
        child.append(&icon);
        child.append(&badge);
        button.set_child(Some(&child));
        let title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        title.add_css_class("notification-title");
        let body = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .max_width_chars(48)
            .lines(3)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.append(&title);
        content.append(&body);
        let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        card.add_css_class("notification-banner");
        card.set_margin_top(12);
        card.set_margin_end(12);
        card.set_margin_start(12);
        card.append(&content);
        let open = gtk::Button::from_icon_name("lucide-bell-symbolic");
        open.set_tooltip_text(Some("Open notifications"));
        open.set_action_name(Some("win.notifications"));
        open.add_css_class("flat");
        let close = gtk::Button::from_icon_name("lucide-x-symbolic");
        close.set_tooltip_text(Some("Dismiss banner"));
        close.add_css_class("flat");
        card.append(&open);
        card.append(&close);
        let banner = gtk::Revealer::builder()
            .child(&card)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(160)
            .build();
        overlay.add_overlay(&banner);
        let this = Rc::new(Self {
            events: RefCell::new(VecDeque::new()),
            next: Cell::new(0),
            badge,
            banner,
            banner_title: title,
            banner_body: body,
            timeout: RefCell::new(None),
            list: RefCell::new(None),
            last_web: Cell::new(None),
            parent: parent.downgrade(),
            desktop: RefCell::new(None),
            open: RefCell::new(None),
            last_notice: RefCell::new(None),
        });
        let weak = Rc::downgrade(&this);
        close.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.hide_banner();
            }
        });
        let weak = Rc::downgrade(&this);
        *this.desktop.borrow_mut() = Some(crate::desktop_notifications::DesktopNotifications::new(
            desktop_enabled,
            move |id| {
                if let Some(this) = weak.upgrade() {
                    if let Some(parent) = this.parent.upgrade() {
                        parent.present();
                    }
                    let action = this
                        .events
                        .borrow_mut()
                        .iter_mut()
                        .find(|e| e.id == id)
                        .and_then(|e| {
                            e.unread = false;
                            e.action.clone()
                        });
                    this.update_badge();
                    if let Some(action) = action {
                        action();
                    } else {
                        let open = this.open.borrow().clone();
                        if let Some(open) = open {
                            open();
                        }
                    }
                }
            },
        ));
        this
    }
    pub fn on_open(&self, open: impl Fn() + 'static) {
        *self.open.borrow_mut() = Some(Rc::new(open));
    }
    pub fn set_desktop(&self, enabled: bool) {
        if let Some(desktop) = self.desktop.borrow().as_ref() {
            desktop.set_enabled(enabled);
        }
    }
    pub fn desktop_status(&self) -> String {
        self.desktop
            .borrow()
            .as_ref()
            .map(|d| d.status())
            .unwrap_or_default()
    }
    pub fn desktop_preview(&self) {
        if let Some(desktop) = self.desktop.borrow().as_ref() {
            desktop.send(
                0,
                "Paisley desktop notifications",
                "You’ll see background alerts here while Paisley is running.",
            );
        }
    }
    pub fn push(
        self: &Rc<Self>,
        title: &str,
        body: &str,
        source: &str,
        action: Option<Rc<dyn Fn()>>,
        web: Option<webkit::Notification>,
    ) {
        if web.is_some() {
            if self
                .last_web
                .get()
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
            {
                if let Some(web) = web {
                    web.close();
                }
                return;
            }
            self.last_web.set(Some(Instant::now()));
        }
        if web.is_none() {
            if self
                .last_notice
                .borrow()
                .as_ref()
                .is_some_and(|(t, b, at)| {
                    t == title && b == body && at.elapsed() < Duration::from_secs(3)
                })
            {
                return;
            }
            *self.last_notice.borrow_mut() =
                Some((text(title, 160), text(body, 600), Instant::now()));
        }
        let reading = self
            .list
            .borrow()
            .as_ref()
            .and_then(|w| w.upgrade())
            .is_some_and(|w| w.is_mapped());
        let id = self.next.get().wrapping_add(1);
        self.next.set(id);
        let title = text(title, 160);
        let body = text(body, 600);
        if let Some(web) = web.as_ref() {
            let weak = Rc::downgrade(self);
            web.connect_closed(move |_| {
                let weak = weak.clone();
                gtk::glib::idle_add_local_once(move || {
                    if let Some(this) = weak.upgrade() {
                        this.remove(id, false);
                    }
                });
            });
        }
        let time = gtk::glib::DateTime::now_local()
            .ok()
            .and_then(|d| d.format("%I:%M %p").ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let old = {
            let mut events = self.events.borrow_mut();
            events.push_front(Event {
                id,
                unread: !reading,
                title: title.clone(),
                body: body.clone(),
                source: text(source, 180),
                time,
                action,
                web,
            });
            if events.len() > 50 {
                events.pop_back()
            } else {
                None
            }
        };
        if let Some(old) = old {
            if let Some(desktop) = self.desktop.borrow().as_ref() {
                desktop.close(old.id);
            }
            if let Some(web) = old.web {
                web.close();
            }
        }
        if self.parent.upgrade().is_some_and(|w| !w.is_active())
            && let Some(desktop) = self.desktop.borrow().as_ref()
        {
            desktop.send(id, &title, &body);
        }
        self.update_badge();
        self.render();
        self.hide_banner();
        if !reading {
            self.banner_title.set_text(&title);
            self.banner_body.set_text(&body);
            self.banner.set_reveal_child(true);
            let weak = Rc::downgrade(self);
            *self.timeout.borrow_mut() = Some(gtk::glib::timeout_add_local_once(
                Duration::from_secs(6),
                move || {
                    if let Some(this) = weak.upgrade() {
                        this.timeout.borrow_mut().take();
                        this.banner.set_reveal_child(false);
                    }
                },
            ));
        }
    }
    fn hide_banner(&self) {
        if let Some(timer) = self.timeout.borrow_mut().take() {
            timer.remove();
        }
        self.banner.set_reveal_child(false);
    }
    fn update_badge(&self) {
        let unread = self.events.borrow().iter().filter(|e| e.unread).count();
        self.badge.set_text(&unread.to_string());
        self.badge.set_visible(unread > 0);
    }
    fn remove(self: &Rc<Self>, id: u64, close: bool) {
        if let Some(desktop) = self.desktop.borrow().as_ref() {
            desktop.close(id);
        }
        let removed = {
            let mut events = self.events.borrow_mut();
            events
                .iter()
                .position(|e| e.id == id)
                .and_then(|i| events.remove(i))
        };
        if close && let Some(web) = removed.and_then(|e| e.web) {
            web.close();
        }
        self.update_badge();
        self.render();
    }
    pub fn panel(self: &Rc<Self>) -> gtk::Box {
        for event in self.events.borrow_mut().iter_mut() {
            event.unread = false;
        }
        self.update_badge();
        self.hide_banner();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        heading.append(&crate::download_ui::label(
            "Recent activity",
            "downloads-section",
        ));
        let clear = gtk::Button::with_label("Clear all");
        clear.add_css_class("flat");
        heading.append(&clear);
        root.append(&heading);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
        *self.list.borrow_mut() = Some(list.downgrade());
        root.append(&list);
        let weak = Rc::downgrade(self);
        clear.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                if let Some(desktop) = this.desktop.borrow().as_ref() {
                    desktop.clear();
                }
                let events = this.events.replace(VecDeque::new());
                this.update_badge();
                this.hide_banner();
                this.render();
                for e in events {
                    if let Some(web) = e.web {
                        web.close();
                    }
                }
            }
        });
        self.render();
        root
    }
    fn render(self: &Rc<Self>) {
        let Some(list) = self.list.borrow().as_ref().and_then(|w| w.upgrade()) else {
            return;
        };
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        if self.events.borrow().is_empty() {
            let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
            empty.set_margin_top(24);
            empty.set_margin_bottom(24);
            let icon = gtk::Image::from_icon_name("lucide-bell-symbolic");
            icon.set_pixel_size(32);
            icon.add_css_class("dim-label");
            empty.append(&icon);
            empty.append(&gtk::Label::new(Some("You’re all caught up")));
            list.append(&empty);
            return;
        }
        for event in self.events.borrow().iter() {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
            card.add_css_class("notification-card");
            let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let title = crate::download_ui::label(&event.title, "notification-title");
            heading.append(&title);
            let dismiss = gtk::Button::from_icon_name("lucide-x-symbolic");
            dismiss.add_css_class("flat");
            dismiss.set_tooltip_text(Some("Dismiss notification"));
            let id = event.id;
            let weak = Rc::downgrade(self);
            dismiss.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.remove(id, true);
                }
            });
            heading.append(&dismiss);
            card.append(&heading);
            let detail = gtk::Label::builder()
                .label(&event.body)
                .xalign(0.0)
                .wrap(true)
                .build();
            detail.set_selectable(true);
            card.append(&detail);
            card.append(&crate::download_ui::label(
                &format!("{} · {}", event.source, event.time),
                "downloads-detail",
            ));
            if event.action.is_some() {
                let open = gtk::Button::with_label(if event.web.is_some() {
                    "Open tab"
                } else {
                    "View"
                });
                open.add_css_class("flat");
                let weak = Rc::downgrade(self);
                open.connect_clicked(move |_| {
                    if let Some(this) = weak.upgrade() {
                        let action = this
                            .events
                            .borrow()
                            .iter()
                            .find(|e| e.id == id)
                            .and_then(|e| e.action.clone());
                        if let Some(action) = action {
                            action();
                        }
                    }
                });
                card.append(&open);
            }
            list.append(&card);
        }
    }
}
impl Drop for Notifications {
    fn drop(&mut self) {
        if let Some(timer) = self.timeout.get_mut().take() {
            timer.remove();
        }
        for event in self.events.get_mut().drain(..) {
            if let Some(web) = event.web {
                web.close();
            }
        }
    }
}
